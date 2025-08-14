use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use dropbear_engine::{camera::Camera, entity::{AdoptedEntity, Transform}, gilrs::{Button, GamepadId}, graphics::{Graphics, Shader}, input::{Controller, Keyboard, Mouse}, scene::{Scene, SceneCommand}, wgpu::{Color, RenderPipeline}, winit::{dpi::PhysicalPosition, event::MouseButton, event_loop::ActiveEventLoop, keyboard::KeyCode, window::Window}, WindowConfiguration};
use egui::Image;
use eucalyptus::{camera::CameraType, scripting::ScriptManager, states::{RuntimeData, SceneConfig, ScriptComponent}};
use eucalyptus::camera::CameraManager;

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

    println!("Loading runtime data from: {}", init_eupak_path.display());

    // decode that content
    let bytes = std::fs::read(&init_eupak_path)?;
    let (content, _): (RuntimeData, usize) = bincode::decode_from_slice(&bytes, bincode::config::standard())?;
    
    println!("Loaded {} scenes", content.scene_data.len());

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
    scene_command: SceneCommand,
    input_state: eucalyptus::scripting::input::InputState,
    render_pipeline: Option<RenderPipeline>,
    window: Option<Arc<Window>>,
    is_cursor_locked: bool,
    camera: Camera,
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
            scene_command: SceneCommand::None,
            input_state: eucalyptus::scripting::input::InputState::new(),
            render_pipeline: None,
            window: None,
            is_cursor_locked: true,
            camera: Camera::default(),
        }
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
        let shader = Shader::new(
            graphics,
            include_str!("../../eucalyptus/src/shader.wgsl"),
            Some("default"),
        );

        // let horse_model =
        //     AdoptedEntity::new(graphics, "models/low_poly_horse.glb", Some("horse")).unwrap();

        // self.world.spawn((horse_model, Transform::default()));

        let camera = Camera::predetermined(graphics);

        let pipeline = graphics.create_render_pipline(
            &shader,
            vec![&graphics.state.texture_bind_layout.clone(), camera.layout()],
        );

        self.camera = camera;
        self.window = Some(graphics.state.window.clone());

        // ensure that this is the last line
        self.render_pipeline = Some(pipeline);
    }

    fn update(&mut self, dt: f32, graphics: &mut Graphics) {
        for key in &self.input_state.pressed_keys {
            match key {
                KeyCode::KeyW => self.camera.move_forwards(),
                KeyCode::KeyA => self.camera.move_left(),
                KeyCode::KeyD => self.camera.move_right(),
                KeyCode::KeyS => self.camera.move_back(),
                KeyCode::ShiftLeft => self.camera.move_down(),
                KeyCode::Space => self.camera.move_up(),
                _ => {}
            }
        }

        if !self.is_cursor_locked {
            self.window.as_mut().unwrap().set_cursor_visible(true);
        }

        // let query = self.world.query_mut::<(&mut AdoptedEntity, &Transform)>();
        // for (_, (entity, transform)) in query {
        //     entity.update(&graphics, transform);
        // }

        self.camera.update(graphics);
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
        
        // ui.scope_builder(egui::UiBuilder::new().max_rect(image_rect), |ui| {
        //             ui.add_sized(
        //                 [display_width, display_height],
        //                 egui::Image::new((
        //                     self.view,
        //                     [display_width, display_height].into(),
        //                 ))
        //                 .fit_to_exact_size([display_width, display_height].into())
        //             )
        //         });

        if let Some(pipeline) = &self.render_pipeline {
            {
                // let mut query = self.world.query::<(&AdoptedEntity, &Transform)>();
                let mut render_pass = graphics.clear_colour(color);
                render_pass.set_pipeline(pipeline);

                // for (_, (entity, _)) in query.iter() {
                //     entity.render(&mut render_pass, &self.camera);
                // }
            }
        }

        self.window = Some(graphics.state.window.clone());
    }


    fn exit(&mut self, _event_loop: &ActiveEventLoop) {
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
    fn mouse_move(&mut self, _position: PhysicalPosition<f64>) {
    }

    fn mouse_down(&mut self, _button: MouseButton) {
    }

    fn mouse_up(&mut self, _button: MouseButton) {
    }
}

impl Controller for RuntimeScene {
    fn button_down(&mut self, _button: dropbear_engine::gilrs::Button, _id: dropbear_engine::gilrs::GamepadId) {
    }

    fn button_up(&mut self, _button: dropbear_engine::gilrs::Button, _id: dropbear_engine::gilrs::GamepadId) {
    }

    fn left_stick_changed(&mut self, _x: f32, _y: f32, _id: dropbear_engine::gilrs::GamepadId) {
    }

    fn right_stick_changed(&mut self, _x: f32, _y: f32, _id: dropbear_engine::gilrs::GamepadId) {
    }

    fn on_connect(&mut self, _id: dropbear_engine::gilrs::GamepadId) {
    }

    fn on_disconnect(&mut self, _id: dropbear_engine::gilrs::GamepadId) {
    }
}
