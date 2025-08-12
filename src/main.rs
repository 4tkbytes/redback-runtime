use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use dropbear_engine::{entity::{AdoptedEntity, Transform}, gilrs::{Button, GamepadId}, graphics::Graphics, input::{Controller, Keyboard, Mouse}, scene::{Scene, SceneCommand}, wgpu::{Color, RenderPipeline}, winit::{dpi::PhysicalPosition, event::MouseButton, event_loop::ActiveEventLoop, keyboard::KeyCode, window::Window}, WindowConfiguration};
use eucalyptus::states::{RuntimeData, SceneConfig};

fn main() -> anyhow::Result<()> {    
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

    log::info!("Loading runtime data from: {}", init_eupak_path.display()); // Debug print

    // decode that content
    let bytes = std::fs::read(&init_eupak_path)?;
    let (content, _): (RuntimeData, usize) = bincode::decode_from_slice(&bytes, bincode::config::standard())?;
    
    log::debug!("Loaded {} scenes", content.scene_data.len());

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
    scene_data: Vec<SceneConfig>,
    scripts: HashMap<String, String>,
    current_scene_index: usize,
    loaded_world: Option<hecs::World>,
    camera_manager: Option<eucalyptus::camera::CameraManager>,
    script_manager: Option<eucalyptus::scripting::ScriptManager>,
    scene_command: SceneCommand,
    input_state: eucalyptus::scripting::input::InputState,
    render_pipeline: Option<RenderPipeline>,
    window: Option<Arc<Window>>,
    is_cursor_locked: bool,
}

impl RuntimeScene {
    fn new(runtime_data: RuntimeData) -> Self {
        Self {
            scene_data: runtime_data.scene_data,
            scripts: runtime_data.scripts,
            current_scene_index: 0,
            loaded_world: None,
            camera_manager: None,
            script_manager: None,
            scene_command: SceneCommand::None,
            input_state: eucalyptus::scripting::input::InputState::new(),
            render_pipeline: None,
            window: None,
            is_cursor_locked: true,
        }
    }

    fn load_scene(&mut self, scene_index: usize, graphics: &Graphics) -> anyhow::Result<()> {
        if scene_index >= self.scene_data.len() {
            return Err(anyhow::anyhow!("Scene index {} out of bounds", scene_index));
        }
        let scene = &self.scene_data[scene_index];
        
        let mut world = hecs::World::new();
        scene.load_into_world(&mut world, graphics)?;
        
        let mut camera_manager = eucalyptus::camera::CameraManager::new();
        scene.load_cameras_into_manager(&mut camera_manager, graphics, &world)?;
        camera_manager.set_active(eucalyptus::camera::CameraType::Player);
        
        let script_manager = eucalyptus::scripting::ScriptManager::new();
        
        for (script_name, script_content) in &self.scripts {
            log::info!("Script available: {}", script_name);
        }
        
        self.loaded_world = Some(world);
        self.camera_manager = Some(camera_manager);
        self.script_manager = Some(script_manager);
        self.current_scene_index = scene_index;
        
        log::info!("Loaded scene: {}", scene.scene_name);
        Ok(())
    }

    fn switch_scene(&mut self, scene_name: &str, graphics: &Graphics) -> anyhow::Result<()> {
        if let Some(index) = self.scene_data.iter().position(|s| s.scene_name == scene_name) {
            self.load_scene(index, graphics)?;
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
        todo!()
    }

    fn update(&mut self, dt: f32, graphics: &mut Graphics) {
        todo!()
    }

    fn render(&mut self, graphics: &mut Graphics) {
        todo!()
    }

    fn exit(&mut self, event_loop: &ActiveEventLoop) {
        todo!()
    }
}

impl Keyboard for RuntimeScene {
    fn key_down(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        todo!()
    }

    fn key_up(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        todo!()
    }
}

impl Mouse for RuntimeScene {
    fn mouse_move(&mut self, position: PhysicalPosition<f64>) {
        todo!()
    }

    fn mouse_down(&mut self, button: MouseButton) {
        todo!()
    }

    fn mouse_up(&mut self, button: MouseButton) {
        todo!()
    }
}

impl Controller for RuntimeScene {
    fn button_down(&mut self, button: dropbear_engine::gilrs::Button, id: dropbear_engine::gilrs::GamepadId) {
        todo!()
    }

    fn button_up(&mut self, button: dropbear_engine::gilrs::Button, id: dropbear_engine::gilrs::GamepadId) {
        todo!()
    }

    fn left_stick_changed(&mut self, x: f32, y: f32, id: dropbear_engine::gilrs::GamepadId) {
        todo!()
    }

    fn right_stick_changed(&mut self, x: f32, y: f32, id: dropbear_engine::gilrs::GamepadId) {
        todo!()
    }

    fn on_connect(&mut self, id: dropbear_engine::gilrs::GamepadId) {
        todo!()
    }

    fn on_disconnect(&mut self, id: dropbear_engine::gilrs::GamepadId) {
        todo!()
    }
}
