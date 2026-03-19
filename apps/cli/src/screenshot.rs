use crate::output::{OutputFormat, print_json_value, status_message};
use cap_recording::screen_capture::ScreenCaptureTarget;
use clap::Args;
use scap_targets::{DisplayId, WindowId};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Args)]
pub struct ScreenshotArgs {
    #[arg(long, group = "target")]
    screen: Option<DisplayId>,

    #[arg(long, group = "target")]
    window: Option<WindowId>,

    #[arg(long, short)]
    output: Option<PathBuf>,
}

#[derive(Serialize)]
struct ScreenshotResult {
    path: String,
    width: u32,
    height: u32,
}

impl ScreenshotArgs {
    pub async fn run(self, format: OutputFormat) -> Result<(), String> {
        let target = match (self.screen, self.window) {
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

        status_message("Capturing screenshot...");

        let image = cap_recording::screenshot::capture_screenshot(target)
            .await
            .map_err(|e| format!("Screenshot failed: {e}"))?;

        let output_path = self.output.unwrap_or_else(|| {
            let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
            std::env::current_dir()
                .unwrap()
                .join(format!("cap-screenshot-{timestamp}.png"))
        });

        let width = image.width();
        let height = image.height();

        image
            .save(&output_path)
            .map_err(|e| format!("Failed to save screenshot: {e}"))?;

        let result = ScreenshotResult {
            path: output_path.display().to_string(),
            width,
            height,
        };

        print_json_value(format, &result, || {
            println!(
                "Screenshot saved to '{}' ({}x{})",
                result.path, result.width, result.height
            );
        });

        Ok(())
    }
}
