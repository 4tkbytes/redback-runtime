use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use dropbear_engine::{camera::Camera, entity::{AdoptedEntity, Transform}, gilrs::{Button, GamepadId}, graphics::{Graphics, Shader}, input::{Controller, Keyboard, Mouse}, scene::{Scene, SceneCommand}, wgpu::{Color, RenderPipeline}, winit::{dpi::PhysicalPosition, event::MouseButton, event_loop::ActiveEventLoop, keyboard::KeyCode, window::Window}, WindowConfiguration};
use eucalyptus::{camera::{CameraType, CameraManager}, scripting::{ScriptManager, input::InputState}, states::{RuntimeData, SceneConfig, ScriptComponent}};

fn main() -> anyhow::Result<()> {    
    if std::env::var("RUST_LOG").is_err() {
        unsafe {
            std::env::set_var("RUST_LOG", "info,redback_runtime=debug");
        }
    }
    let _ = env_logger::try_init();

    // ensure that {project_name}.eupak exists in same dir as runtime
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

    // decode that content
    let bytes = std::fs::read(&init_eupak_path)?;
    let (content, _): (RuntimeData, usize) = bincode::decode_from_slice(&bytes, bincode::config::standard())?;
    
    log::info!("Loaded {} scenes", content.scene_data.len());

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
    scripts: HashMap<String, String>,
    current_scene_name: String,
    world: hecs::World,
    camera_manager: CameraManager,
    script_manager: ScriptManager,
    scene_command: SceneCommand,
    input_state: InputState,
    render_pipeline: Option<RenderPipeline>,
    window: Option<Arc<Window>>,
    is_cursor_locked: bool,
}

impl RuntimeScene {
    fn new(runtime_data: RuntimeData) -> Self {
        let mut scene_data = HashMap::new();
        for data in &runtime_data.scene_data {
            scene_data.insert(data.scene_name.clone(), data.clone());
        }

        Self {
            scene_data,
            scripts: runtime_data.scripts,
            current_scene_name: String::new(),
            world: hecs::World::new(),
            camera_manager: CameraManager::new(),
            script_manager: ScriptManager::new(),
            scene_command: SceneCommand::None,
            input_state: InputState::new(),
            render_pipeline: None,
            window: None,
            is_cursor_locked: true,
        }
    }

    fn load_scene(&mut self, graphics: &mut Graphics, scene_name: String) -> anyhow::Result<()> {
        let scene_config = self.scene_data.get(&scene_name)
            .ok_or_else(|| anyhow::anyhow!("Scene '{}' not found", scene_name))?
            .clone();

        log::info!("Loading scene: {}", scene_name);
        
        self.world.clear();
        self.camera_manager.clear_cameras();
        
        for entity_config in &scene_config.entities {
            log::debug!("Loading entity: {}", entity_config.label);

            let adopted = AdoptedEntity::new(
                graphics,
                &entity_config.model_path,
                Some(&entity_config.label),
            )?;

            let transform = entity_config.transform;
            let properties = entity_config.properties.clone();

            if let Some(script_config) = &entity_config.script {
                let script = ScriptComponent {
                    name: script_config.name.clone(),
                    path: script_config.path.clone(),
                };
                self.world.spawn((adopted, transform, properties, script));
            } else {
                self.world.spawn((adopted, transform, properties));
            }
        }

        if let Err(e) = scene_config.load_cameras_into_manager(&mut self.camera_manager, graphics, &self.world) {
            log::warn!("Failed to load cameras from scene config: {}", e);
            
            let debug_camera = Camera::predetermined(graphics);
            let debug_controller = Box::new(eucalyptus::camera::DebugCameraController::new());
            self.camera_manager.add_camera(CameraType::Debug, debug_camera, debug_controller);
            self.camera_manager.set_active(CameraType::Debug);
            log::info!("Created fallback debug camera");
        } else {
            if self.camera_manager.get_camera(&CameraType::Player).is_some() {
                log::info!("Setting Player camera as active");
                self.camera_manager.set_active(CameraType::Player);
            } else if self.camera_manager.get_camera(&CameraType::Debug).is_some() {
                log::info!("Setting Debug camera as active");
                self.camera_manager.set_active(CameraType::Debug);
            }
        }

        let script_entities: Vec<_> = self.world.query::<&ScriptComponent>().iter()
            .map(|(entity_id, script)| (entity_id, script.name.clone()))
            .collect();

        for (entity_id, script_name) in script_entities {
            if let Some(script_content) = self.scripts.get(&script_name) {
                if let Err(e) = self.script_manager.load_script_from_source(&script_name, script_content) {
                    log::warn!("Failed to load script '{}': {}", script_name, e);
                    continue;
                }
                
                if let Err(e) = self.script_manager.init_entity_script(entity_id, &script_name, &mut self.world, &self.input_state) {
                    log::warn!("Failed to initialize script '{}' for entity {:?}: {}", script_name, entity_id, e);
                }
            } else {
                log::warn!("Script content not found for '{}'", script_name);
            }
        }

        self.current_scene_name = scene_name;
        log::info!("Scene loaded successfully with {} entities", self.world.len());

        Ok(())
    }

    #[allow(dead_code)] //remove this, give it a purpose later
    fn switch_scene(&mut self, graphics: &mut Graphics, scene_name: String) -> anyhow::Result<()> {
        if self.scene_data.contains_key(&scene_name) {
            for (entity_id, _) in self.world.query::<&ScriptComponent>().iter() {
                self.script_manager.remove_entity_script(entity_id);
            }
            
            self.load_scene(graphics, scene_name)?;
        } else {
            return Err(anyhow::anyhow!("Scene '{}' not found", scene_name));
        }
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
        let scene_name = if self.scene_data.contains_key("Default") {
            "Default".to_string()
        } else if let Some(first_scene) = self.scene_data.keys().next() {
            first_scene.clone()
        } else {
            log::error!("No scenes available to load");
            return;
        };

        if let Err(e) = self.load_scene(graphics, scene_name) {
            log::error!("Failed to load scene: {}", e);
            return;
        }

        let shader = Shader::new(
            graphics,
            include_str!("shader.wgsl"),
            Some("runtime_shader"),
        );

        let texture_bind_group = &graphics.texture_bind_group().clone();
        let model_layout = graphics.create_model_uniform_bind_group_layout();

        if let Some(camera) = self.camera_manager.get_camera_mut(&CameraType::Player) {
            camera.aspect = (graphics.screen_size.0 / graphics.screen_size.1) as f64;
            let pipeline = graphics.create_render_pipline(
                &shader,
                vec![texture_bind_group, camera.layout(), &model_layout],
            );
            self.render_pipeline = Some(pipeline);
            log::info!("Render pipeline created successfully");
        } else {
            log::error!("No active camera found, cannot create render pipeline");
        }

        self.window = Some(graphics.state.window.clone());
    }

    fn update(&mut self, dt: f32, graphics: &mut Graphics) {
        self.camera_manager.update_camera_following(&self.world, dt);
        self.camera_manager.update_all(dt, graphics);

        let script_entities: Vec<_> = self.world.query::<&ScriptComponent>().iter()
            .map(|(entity_id, script)| (entity_id, script.name.clone()))
            .collect();

        for (entity_id, script_name) in script_entities {
            if let Err(e) = self.script_manager.update_entity_script(entity_id, &script_name, &mut self.world, &self.input_state, dt) {
                log::warn!("Failed to update script '{}' for entity {:?}: {}", script_name, entity_id, e);
            }
        }

        let query = self.world.query_mut::<(&mut AdoptedEntity, &Transform)>();
        for (_, (entity, transform)) in query {
            entity.update(graphics, transform);
        }

        if !self.is_cursor_locked {
            if let Some(window) = &self.window {
                window.set_cursor_visible(true);
            }
        }
    }

    fn render(&mut self, graphics: &mut Graphics) {
        let color = Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 1.0,
        };

        let texture_id = graphics.state.texture_id.clone();
        let (display_width, display_height) = graphics.screen_size;
        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(graphics.get_egui_context(), |ui| {
                let rect = ui.max_rect();
                ui.put(
                    rect,
                    egui::Image::new((texture_id, [display_width, display_height].into()))
                        .fit_to_exact_size([display_width, display_height].into()),
                );
            });

        if let Some(pipeline) = &self.render_pipeline {
            if let Some(camera) = self.camera_manager.get_active() {
                let mut query = self.world.query::<(&AdoptedEntity, &Transform)>();
                let mut render_pass = graphics.clear_colour(color);
                render_pass.set_pipeline(pipeline);

                for (_, (entity, _)) in query.iter() {
                    entity.render(&mut render_pass, camera);
                }
            }
        }

        self.window = Some(graphics.state.window.clone());
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
        self.input_state.pressed_keys.insert(key);
    }

    fn key_up(&mut self, key: KeyCode, _event_loop: &ActiveEventLoop) {
        self.input_state.pressed_keys.remove(&key);
    }
}

impl Mouse for RuntimeScene {
    fn mouse_move(&mut self, position: PhysicalPosition<f64>) {
        self.input_state.mouse_pos = position.into();
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