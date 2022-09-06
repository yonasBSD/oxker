#![forbid(unsafe_code)]
#![warn(clippy::unused_async, clippy::unwrap_used, clippy::expect_used)]
// Wanring - These are indeed pedantic
// #![warn(clippy::pedantic)]
// #![warn(clippy::nursery)]
// #![allow(clippy::module_name_repetitions, clippy::doc_markdown)]

// Only allow when debugging
// #![allow(unused)]

use app_data::AppData;
use app_error::AppError;
use bollard::Docker;
use docker_data::DockerData;
use parking_lot::Mutex;
use parse_args::CliArgs;
use std::sync::{atomic::AtomicBool, Arc};
use tracing::{info, Level};

mod app_data;
mod app_error;
mod docker_data;
mod input_handler;
mod parse_args;
mod ui;

use ui::{create_ui, GuiState};

fn setup_tracing() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
	// TODO write to file?
}

#[tokio::main]
async fn main() {
    setup_tracing();
    let args = CliArgs::new();
    let app_data = Arc::new(Mutex::new(AppData::default(args.clone())));
    let gui_state = Arc::new(Mutex::new(GuiState::default()));
    let is_running = Arc::new(AtomicBool::new(true));

    let docker_args = args.clone();
    let docker_app_data = Arc::clone(&app_data);
    let docker_gui_state = Arc::clone(&gui_state);

    let (docker_sx, docker_rx) = tokio::sync::mpsc::channel(16);

    // Create docker daemon handler, and only spawn up the docker data handler if ping returns non-error

    match Docker::connect_with_socket_defaults() {
        Ok(docker) => {
            let docker = Arc::new(docker);
            match docker.ping().await {
                Ok(_) => {
                    let docker = Arc::clone(&docker);
                    let is_running = Arc::clone(&is_running);
                    tokio::spawn(DockerData::init(
                        docker_args,
                        docker_app_data,
                        docker,
                        docker_gui_state,
                        docker_rx,
                        is_running,
                    ));
                }
                Err(_) => app_data.lock().set_error(AppError::DockerConnect),
            }
        }
        Err(_) => app_data.lock().set_error(AppError::DockerConnect),
    }
    let input_app_data = Arc::clone(&app_data);

    let (input_sx, input_rx) = tokio::sync::mpsc::channel(16);

    let input_is_running = Arc::clone(&is_running);
    let input_gui_state = Arc::clone(&gui_state);
    let input_docker_sender = docker_sx.clone();

    // Spawn input handling into own tokio thread
    tokio::spawn(input_handler::InputHandler::init(
        input_app_data,
        input_rx,
        input_docker_sender,
        input_gui_state,
        input_is_running,
    ));

    // Debug mode for testing, mostly pointless, doesn't take terminal nor draw gui
    if args.gui {
        let update_duration = std::time::Duration::from_millis(u64::from(args.docker_interval));
        create_ui(
            app_data,
            input_sx,
            is_running,
            gui_state,
            docker_sx,
            update_duration,
        )
        .await
        .unwrap_or(());
    } else {
        loop {
			// TODO this needs to be improved to display something useful
            info!("in debug mode");
            tokio::time::sleep(std::time::Duration::from_millis(5000)).await;
        }
    }
}
