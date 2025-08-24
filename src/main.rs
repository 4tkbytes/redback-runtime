#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use bincode::error::DecodeError;
use dropbear_engine::{entity::{AdoptedEntity, Transform}, gilrs::{Button, GamepadId}, graphics::{Graphics, Shader}, input::{Controller, Keyboard, Mouse}, lighting::{Light, LightManager, LightType}, scene::{Scene, SceneCommand}, wgpu::{Color, RenderPipeline}, WindowConfiguration};
use glam::DVec3;
use rfd::{MessageButtons, MessageDialogResult, MessageLevel};
use winit::{dpi::PhysicalPosition, event::MouseButton, event_loop::ActiveEventLoop, keyboard::KeyCode, window::Window};
use dropbear_engine::lighting::LightComponent;
use dropbear_engine::model::{DrawLight, DrawModel};
use eucalyptus::{camera::CameraManager, scripting::{ScriptManager, input::InputState}, states::{RuntimeData, SceneConfig, ScriptComponent}};

fn main() -> anyhow::Result<()> {
    #[cfg(not(target_os = "android"))]
    {
        let app_name = env!("CARGO_BIN_NAME");
        if cfg!(debug_assertions) {
            log::info!("Running in dev mode");
            let app_target = app_name.replace('-', "_");
            let log_config = format!("dropbear_engine=trace,{}=debug,warn, eucalyptus=debug,warn", app_target);
            unsafe { std::env::set_var("RUST_LOG", log_config) };
        }
        env_logger::init();
    }

    run()?;
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let file_name = current_exe
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Unable to get file name"))?;
    let binding = file_name
        .to_string_lossy();
    let project_name = binding
        .strip_suffix(".exe")
        .ok_or_else(|| anyhow::anyhow!("Unable to strip suffix while fetching the executable's name: {}", file_name.display()))?
        .to_string();
    let exe_dir = current_exe.parent().ok_or_else(|| anyhow::anyhow!("Failed to get executable directory"))?;
    let init_eupak_path = exe_dir.join(format!("{}.eupak", project_name));
    if !init_eupak_path.exists() {
        return Err(anyhow::anyhow!("{}.eupak was not found at {}, which is required to start the game.", project_name, init_eupak_path.display()));
    }

    log::info!("Loading runtime data from: {}", init_eupak_path.display());

    let bytes = std::fs::read(&init_eupak_path)?;
    let (content, _): (RuntimeData, usize) = match bincode::decode_from_slice(&bytes, bincode::config::standard()) {
            Ok((content, len)) => (content, len),
            Err(e) if matches!(e, DecodeError::Utf8 { .. }) => {
                log::error!("Uh oh, hit an error attempting to decode {}...", init_eupak_path.display());
                let dialogue = rfd::MessageDialog::new()
                    .set_title("Error loading game")
                    .set_description("Your game .eupak package is outdated and cannot be read with the latest redback-runtime executable, which means you \
                    miss out on features that can be crucial to a game. \n\nPlease either update your game, use a supported redback-runtime version \
                    or report this issue to the developer. \n\n\
                    Logs are attached in [TEMP LOG LOCATION PLACEHOLDER], so send that to them too! \
                    \n\nGood Luck...")
                    .set_buttons(MessageButtons::Ok)
                    .set_level(MessageLevel::Error)
                    .show();
                match dialogue {
                    MessageDialogResult::Ok => {panic!("Error loading package: {e}\n\nPlease report this to the game developer!")}
                    _ => {panic!("Error loading package: {e}\n\nPlease report this to the game developer!\n\n")}
                }
            }
            Err(e) => {
                log::error!("Uh oh, hit an error attempting to decode {}...", init_eupak_path.display());
                let dialogue = rfd::MessageDialog::new()
                    .set_title("Error loading game")
                    .set_description(format!("Error loading package: {}", e))
                    .set_buttons(MessageButtons::Ok)
                    .set_level(MessageLevel::Error)
                    .show();
                match dialogue {
                    MessageDialogResult::Ok => {panic!("Error loading package: {e}\nPlease report this to the game developer!\n\n")}
                    _ => {panic!("Error loading package: {e}\nPlease report this to the game developer!\n\n")}
                }
            }
    };
    
    log::info!("Loaded {} scenes", content.scene_data.len());

    log::debug!("Runtime Data: {:#?}", content);

    let config = WindowConfiguration {
        windowed_mode: dropbear_engine::WindowedModes::Maximised,
        title: project_name.clone(),
        max_fps: 60,
    };

    dropbear_engine::run_app!(config, |sm, im| {
        setup_from_runtime_data(sm, im, content)
    }).unwrap();

    Ok(())
}

struct RuntimeScene {
    scene_data: HashMap<String, SceneConfig>,
    current_scene_name: String,
    world: hecs::World,
    camera_manager: CameraManager,
    script_manager: ScriptManager,
    light_manager: LightManager,
    scene_command: SceneCommand,
    input_state: InputState,
    render_pipeline: Option<RenderPipeline>,
    window: Option<Arc<Window>>,
}

impl RuntimeScene {
    fn new(runtime_data: RuntimeData) -> Self {
        let mut scene_data = HashMap::new();
        for data in &runtime_data.scene_data {
            scene_data.insert(data.scene_name.clone(), data.clone());
        }

        Self {
            scene_data,
            current_scene_name: String::new(),
            world: hecs::World::new(),
            camera_manager: CameraManager::new(),
            script_manager: ScriptManager::new(),
            light_manager: LightManager::new(),
            scene_command: SceneCommand::None,
            input_state: InputState::new(),
            render_pipeline: None,
            window: None,
        }
    }

    fn load_scene(&mut self, graphics: &mut Graphics, scene_name: impl Into::<String>) -> anyhow::Result<()> {
        let scene_name: String = scene_name.into();

        self.world.clear();
        self.camera_manager.clear_cameras();

        let scene = self.scene_data.get(&scene_name).ok_or_else(|| anyhow::anyhow!("Unable to fetch scene config: Returned \"None\""))?;

        scene.load_into_world(&mut self.world, graphics)?;
        scene.load_cameras_into_manager(&mut self.camera_manager, graphics,&mut self.world)?;

        let mut script_entities: Vec<(hecs::Entity, ScriptComponent)> = Vec::new();
        for (entity_id, script) in self.world.query::<&ScriptComponent>().iter() {
            script_entities.push((entity_id, script.clone()));
        }

        for (entity_id, script) in script_entities {
            match self.script_manager.load_script(&script.path) {
                Ok(script_name) => {
                    if let Err(e) = self.script_manager.init_entity_script(entity_id, &script_name, &mut self.world, &self.input_state) {
                        log::warn!("Failed to initialise script '{}' for entity {:?}: {}", script.name, entity_id, e);
                    }
                }
                Err(e) => {
                    log::warn!("Failed to load script '{}': {}", script.name, e);
                }
            }
        }
        
        self.current_scene_name = scene_name;
        Ok(())
    }
}

fn setup_from_runtime_data(
    mut scene_manager: dropbear_engine::scene::Manager,
    mut input_manager: dropbear_engine::input::Manager,
    runtime_data: RuntimeData,
) -> (dropbear_engine::scene::Manager, dropbear_engine::input::Manager) {
    let runtime_scene = Rc::new(RefCell::new(RuntimeScene::new(runtime_data)));
    
    dropbear_engine::scene::add_scene_with_input(
        &mut scene_manager,
        &mut input_manager,
        runtime_scene,
        "runtime_game",
    );
    
    scene_manager.switch("runtime_game");
    
    (scene_manager, input_manager)
}

impl Scene for RuntimeScene {
    fn load(&mut self, graphics: &mut Graphics) {
        if let Err(e) = self.load_scene(graphics, "Default") {
            log::error!("Failed to load scene 'Default': {}", e);
        }

        self.camera_manager.set_active(eucalyptus::camera::CameraType::Player);

        let shader = Shader::new(
            graphics,
            include_str!("shader.wgsl"),
            Some("redback_runtime_default"),
        );

        self.light_manager.create_light_array_resources(graphics);
        let texture_bind_group = graphics.texture_bind_group().clone();

        if let Some(camera) = self.camera_manager.get_active() {
            let pipeline = graphics.create_render_pipline(
                &shader,
                vec![
                    &texture_bind_group,
                    camera.layout(),
                    self.light_manager.layout()
                ],
                None,
            );
            self.render_pipeline = Some(pipeline);

            self.light_manager.create_render_pipeline(
                graphics,
                include_str!("light.wgsl"),
                camera,
                Some("Light Pipeline")
            );
        } else {
            panic!("Unable to create render pipeline, which is required for graphics. Please rerun with the logs enabled to figure out the issue or send to the devs!");
        }

        self.window = Some(graphics.state.window.clone());
    }

    fn update(&mut self, dt: f32, graphics: &mut Graphics) {
        if !self.input_state.is_cursor_locked {
            if let Some(window) = &self.window {
                window.set_cursor_visible(true);
            }
        }

        let mut script_entities: Vec<(hecs::Entity, String)> = Vec::new();
        for (entity_id, script) in self.world.query::<&ScriptComponent>().iter() {
            script_entities.push((entity_id, script.name.clone()));
        }

        for (entity_id, script_name) in script_entities {
            if let Err(e) = self
                .script_manager
                .update_entity_script(entity_id, &script_name, &mut self.world, &self.input_state, dt)
            {
                log::warn!(
                    "Failed to update script '{}' for entity {:?}: {}",
                    script_name,
                    entity_id,
                    e
                );
            }
        }

        self.camera_manager.update_camera_following(&self.world, dt);

        self.camera_manager.update_all(dt, graphics);

        let query = self.world.query_mut::<(&mut AdoptedEntity, &Transform)>();
        for (_, (entity, transform)) in query {
            entity.update(graphics, transform);
        }

        self.input_state.mouse_delta = None;
    }

    fn render(&mut self, graphics: &mut Graphics) {
        // cornflower blue
        let color = Color {
            r: 100.0 / 255.0,
            g: 149.0 / 255.0,
            b: 237.0 / 255.0,
            a: 1.0,
        };

        self.window = Some(graphics.state.window.clone());
        if let Some(pipeline) = &self.render_pipeline {
            if let Some(camera) = self.camera_manager.get_active() {
                let mut light_query = self.world.query::<(&Light, &LightComponent)>();
                let mut entity_query = self.world.query::<(&AdoptedEntity, &Transform)>();
                {
                    let mut render_pass = graphics.clear_colour(color);
                    if let Some(light_pipeline) = &self.light_manager.pipeline {
                        render_pass.set_pipeline(light_pipeline);
                        for (_, (light, component)) in light_query.iter() {
                            if component.enabled {
                                render_pass.set_vertex_buffer(1, light.instance_buffer.as_ref().unwrap().slice(..));
                                render_pass.draw_light_model(
                                    light.model(),
                                    camera.bind_group(),
                                    light.bind_group(),
                                );
                            }
                        }
                    }

                    render_pass.set_pipeline(pipeline);

                    for (_, (entity, _)) in entity_query.iter() {
                        render_pass.set_vertex_buffer(1, entity.instance_buffer.as_ref().unwrap().slice(..));
                        // render_pass.set_bind_group(2, entity.uniform_bind_group.as_ref().unwrap(), &[]);
                        render_pass.draw_model(entity.model(), camera.bind_group(), self.light_manager.bind_group());
                    }
                }
            }
        }
    }

    fn exit(&mut self, _event_loop: &ActiveEventLoop) {
        for (entity_id, _) in self.world.query::<&ScriptComponent>().iter() {
            self.script_manager.remove_entity_script(entity_id);
        }
    }

    fn run_command(&mut self) -> SceneCommand {
        std::mem::replace(&mut self.scene_command, SceneCommand::None)
    }
}

impl Keyboard for RuntimeScene {
    fn key_down(&mut self, key: KeyCode, _event_loop: &ActiveEventLoop) {
        match key {
            KeyCode::Escape => {
                self.scene_command = SceneCommand::Quit;
            }
            KeyCode::F1 => {
                self.input_state.is_cursor_locked = !self.input_state.is_cursor_locked;
                self.input_state.lock_cursor(self.input_state.is_cursor_locked);
                if let Some(window) = &self.window {
                    window.set_cursor_visible(!self.input_state.is_cursor_locked);
                    if self.input_state.is_cursor_locked {
                        let size = window.inner_size();
                        let center = PhysicalPosition::new(size.width as f64 / 2.0, size.height as f64 / 2.0);
                        let _ = window.set_cursor_position(center);
                    }
                }
            }
            _ => {
                self.input_state.pressed_keys.insert(key);
            }
        }
    }

    fn key_up(&mut self, key: KeyCode, _event_loop: &ActiveEventLoop) {
        self.input_state.pressed_keys.remove(&key);
    }
}

impl Mouse for RuntimeScene {
    fn mouse_move(&mut self, position: PhysicalPosition<f64>) {
        if self.input_state.is_cursor_locked
        {
            if let Some(window) = &self.window {
                let size = window.inner_size();
                let center =
                    PhysicalPosition::new(size.width as f64 / 2.0, size.height as f64 / 2.0);

                let dx = position.x - center.x;
                let dy = position.y - center.y;
                let camera = self.camera_manager.get_active_mut().unwrap();
                camera.track_mouse_delta(dx, dy);

                let _ = window.set_cursor_position(center);
                window.set_cursor_visible(false);
            }
        }
        self.input_state.mouse_pos = (position.x, position.y);
    }

    fn mouse_down(&mut self, button: MouseButton) {
        self.input_state.mouse_button.insert(button);
    }

    fn mouse_up(&mut self, button: MouseButton) {
        self.input_state.mouse_button.remove(&button);
    }
}

impl Controller for RuntimeScene {
    fn button_down(&mut self, _button: Button, _id: GamepadId) {
        // self.input_state.controller_button_down(button, id);
    }

    fn button_up(&mut self, _button: Button, _id: GamepadId) {
        // self.input_state.controller_button_up(button, id);
    }

    fn left_stick_changed(&mut self, _x: f32, _y: f32, _id: GamepadId) {
        // self.input_state.left_stick_changed(x, y, id);
    }

    fn right_stick_changed(&mut self, _x: f32, _y: f32, _id: GamepadId) {
        // self.input_state.right_stick_changed(x, y, id);
    }

    fn on_connect(&mut self, id: GamepadId) {
        log::info!("Controller connected: {:?}", id);
    }

    fn on_disconnect(&mut self, id: GamepadId) {
        log::info!("Controller disconnected: {:?}", id);
    }
}