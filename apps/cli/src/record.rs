use crate::output::{OutputFormat, print_json_value, status_message};
use cap_recording::{screen_capture::ScreenCaptureTarget, studio_recording};
use clap::{Args, ValueEnum};
use scap_targets::{DisplayId, WindowId};
use serde::Serialize;
use std::{env::current_dir, path::PathBuf, time::Instant};
use uuid::Uuid;

#[derive(ValueEnum, Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordingMode {
    Studio,
    Instant,
}

#[derive(Args)]
pub struct RecordStart {
    #[command(flatten)]
    target: RecordTargets,

    #[arg(long)]
    camera: Option<String>,

    #[arg(long)]
    mic: Option<u32>,

    #[arg(long)]
    system_audio: bool,

    #[arg(long)]
    path: Option<PathBuf>,

    #[arg(long)]
    fps: Option<u32>,

    #[arg(long, help = "Recording duration in seconds (auto-stops when reached)")]
    duration: Option<f64>,

    #[arg(long, value_enum, default_value = "studio")]
    mode: RecordingMode,
}

#[derive(Serialize)]
struct RecordingResult {
    project_path: String,
    duration_secs: f64,
    mode: String,
}

impl RecordStart {
    pub async fn run(self, format: OutputFormat) -> Result<(), String> {
        let target_info = match (self.target.screen, self.target.window) {
            (Some(id), _) => cap_recording::screen_capture::list_displays()
                .into_iter()
                .find(|s| s.0.id == id)
                .map(|(s, _)| ScreenCaptureTarget::Display { id: s.id })
                .ok_or(format!("Screen with id '{id}' not found")),
            (_, Some(id)) => cap_recording::screen_capture::list_windows()
                .into_iter()
                .find(|s| s.0.id == id)
                .map(|(s, _)| ScreenCaptureTarget::Window { id: s.id })
                .ok_or(format!("Window with id '{id}' not found")),
            _ => Err("No target specified. Use --screen <id> or --window <id>".to_string()),
        }?;

        let id = Uuid::new_v4().to_string();
        let path = self.path.unwrap_or_else(|| {
            let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
            current_dir()
                .unwrap()
                .join(format!("cap-{timestamp}-{id}.cap"))
        });

        let mode_str = match self.mode {
            RecordingMode::Studio => "studio",
            RecordingMode::Instant => "instant",
        };

        status_message(&format!(
            "Starting {mode_str} recording to '{}'",
            path.display()
        ));

        let system_audio = self.system_audio;
        let fps = self.fps;
        let duration = self.duration;

        match self.mode {
            RecordingMode::Studio => {
                run_studio(target_info, path, format, system_audio, fps, duration).await
            }
            RecordingMode::Instant => {
                run_instant(target_info, path, format, system_audio, duration).await
            }
        }
    }
}

async fn run_studio(
    target_info: ScreenCaptureTarget,
    path: PathBuf,
    format: OutputFormat,
    system_audio: bool,
    fps: Option<u32>,
    duration: Option<f64>,
) -> Result<(), String> {
    let mut builder = studio_recording::Actor::builder(path.clone(), target_info)
        .with_system_audio(system_audio)
        .with_custom_cursor(false);

    if let Some(fps) = fps {
        builder = builder.with_max_fps(fps);
    }

    let actor = builder
        .build(
            #[cfg(target_os = "macos")]
            Some(cap_recording::SendableShareableContent::from(
                cidre::sc::ShareableContent::current()
                    .await
                    .map_err(|e| format!("Failed to get shareable content: {e}"))?,
            )),
        )
        .await
        .map_err(|e| e.to_string())?;

    let start_time = Instant::now();

    if duration.is_some() {
        status_message("Recording... (will auto-stop after specified duration)");
    } else {
        status_message("Recording... (press Ctrl+C to stop)");
    }

    let stop_reason = wait_for_stop_signal(duration, actor.done_fut()).await;

    status_message(&format!("Stopping recording ({stop_reason})..."));

    actor.stop().await.map_err(|e| e.to_string())?;

    let duration_secs = start_time.elapsed().as_secs_f64();

    let result = RecordingResult {
        project_path: path.display().to_string(),
        duration_secs,
        mode: "studio".to_string(),
    };

    print_json_value(format, &result, || {
        println!(
            "Recording saved to '{}' ({:.1}s)",
            result.project_path, result.duration_secs
        );
    });

    Ok(())
}

async fn run_instant(
    target_info: ScreenCaptureTarget,
    path: PathBuf,
    format: OutputFormat,
    system_audio: bool,
    duration: Option<f64>,
) -> Result<(), String> {
    let builder = cap_recording::instant_recording::Actor::builder(path.clone(), target_info)
        .with_system_audio(system_audio);

    let actor = builder
        .build(
            #[cfg(target_os = "macos")]
            Some(cap_recording::SendableShareableContent::from(
                cidre::sc::ShareableContent::current()
                    .await
                    .map_err(|e| format!("Failed to get shareable content: {e}"))?,
            )),
        )
        .await
        .map_err(|e| e.to_string())?;

    let start_time = Instant::now();

    if duration.is_some() {
        status_message("Recording... (will auto-stop after specified duration)");
    } else {
        status_message("Recording... (press Ctrl+C to stop)");
    }

    let stop_reason = wait_for_stop_signal(duration, actor.done_fut()).await;

    status_message(&format!("Stopping recording ({stop_reason})..."));

    actor.stop().await.map_err(|e| e.to_string())?;

    let duration_secs = start_time.elapsed().as_secs_f64();

    let result = RecordingResult {
        project_path: path.display().to_string(),
        duration_secs,
        mode: "instant".to_string(),
    };

    print_json_value(format, &result, || {
        println!(
            "Recording saved to '{}' ({:.1}s)",
            result.project_path, result.duration_secs
        );
    });

    Ok(())
}

async fn wait_for_stop_signal(
    duration: Option<f64>,
    done_fut: cap_recording::DoneFut,
) -> &'static str {
    let duration_sleep = async {
        match duration {
            Some(secs) => tokio::time::sleep(tokio::time::Duration::from_secs_f64(secs)).await,
            None => std::future::pending().await,
        }
    };

    tokio::select! {
        _ = tokio::signal::ctrl_c() => "interrupted",
        _ = duration_sleep => "duration reached",
        _ = done_fut => "recording completed",
    }
}

#[derive(Args)]
struct RecordTargets {
    #[arg(long, group = "target")]
    screen: Option<DisplayId>,

    #[arg(long, group = "target")]
    window: Option<WindowId>,
}
