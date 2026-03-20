mod output;
mod record;
mod screenshot;

use std::{io::Write, path::PathBuf, sync::Arc, sync::atomic::AtomicU32};

use cap_export::ExporterBase;
use cap_project::XY;
use clap::{Args, Parser, Subcommand, ValueEnum};
use output::{OutputFormat, print_error, print_json_value, print_list, status_message};
use record::RecordStart;
use screenshot::ScreenshotArgs;
use serde::Serialize;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "cap", about = "Cap — Screen recording for sharing", version)]
struct Cli {
    #[arg(long, global = true, help = "Output results as JSON")]
    json: bool,

    #[arg(short, long, global = true, help = "Enable verbose logging")]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Export(Export),
    Record(RecordArgs),
    Screenshot(ScreenshotArgs),
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
struct RecordArgs {
    #[command(subcommand)]
    command: Option<RecordCommands>,

    #[command(flatten)]
    args: RecordStart,
}

#[derive(Subcommand)]
enum RecordCommands {
    Screens,
    Windows,
    Cameras,
    Mics,
}

#[derive(Serialize)]
struct ScreenInfo {
    index: usize,
    id: String,
    name: String,
    refresh_rate: f64,
}

#[derive(Serialize)]
struct WindowInfo {
    index: usize,
    id: String,
    name: String,
    owner_name: String,
    refresh_rate: f64,
}

#[derive(Serialize)]
struct CameraInfo {
    display_name: String,
}

#[derive(Serialize)]
struct MicInfo {
    name: String,
}

#[derive(Serialize)]
struct ExportResult {
    output_path: String,
}

#[derive(ValueEnum, Clone, Debug)]
enum CompressionLevel {
    Maximum,
    Social,
    Web,
    Potato,
}

impl From<CompressionLevel> for cap_export::mp4::ExportCompression {
    fn from(val: CompressionLevel) -> Self {
        match val {
            CompressionLevel::Maximum => cap_export::mp4::ExportCompression::Maximum,
            CompressionLevel::Social => cap_export::mp4::ExportCompression::Social,
            CompressionLevel::Web => cap_export::mp4::ExportCompression::Web,
            CompressionLevel::Potato => cap_export::mp4::ExportCompression::Potato,
        }
    }
}

fn parse_resolution(s: &str) -> Result<(u32, u32), String> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() != 2 {
        return Err("Resolution must be in WIDTHxHEIGHT format (e.g., 1920x1080)".to_string());
    }
    let width = parts[0]
        .parse::<u32>()
        .map_err(|_| "Invalid width".to_string())?;
    let height = parts[1]
        .parse::<u32>()
        .map_err(|_| "Invalid height".to_string())?;
    Ok((width, height))
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let cli = Cli::parse();
    let format = OutputFormat::from_json_flag(cli.json);

    let filter = if cli.verbose {
        EnvFilter::new("cap=trace,cap_recording=trace,cap_export=trace,cap_media=trace")
    } else {
        EnvFilter::new("cap=info,cap_recording=warn,cap_export=warn")
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(true)
                .with_target(cli.verbose)
                .with_writer(std::io::stderr),
        )
        .init();

    match cli.command {
        Commands::Export(e) => {
            if let Err(e) = e.run(format).await {
                print_error(format, &e);
                std::process::exit(1);
            }
        }
        Commands::Record(RecordArgs { command, args }) => match command {
            Some(RecordCommands::Screens) => {
                let screens = cap_recording::screen_capture::list_displays();

                let items: Vec<ScreenInfo> = screens
                    .iter()
                    .enumerate()
                    .map(|(i, (screen, target))| ScreenInfo {
                        index: i,
                        id: screen.id.to_string(),
                        name: screen.name.clone(),
                        refresh_rate: target.refresh_rate(),
                    })
                    .collect();

                print_list(format, &items, || {
                    for item in &items {
                        println!(
                            "screen {}:\n  id: {}\n  name: {}\n  fps: {}",
                            item.index, item.id, item.name, item.refresh_rate
                        );
                    }
                });
            }
            Some(RecordCommands::Windows) => {
                let windows = cap_recording::screen_capture::list_windows();

                let items: Vec<WindowInfo> = windows
                    .iter()
                    .enumerate()
                    .map(|(i, (window, target))| WindowInfo {
                        index: i,
                        id: window.id.to_string(),
                        name: window.name.clone(),
                        owner_name: window.owner_name.clone(),
                        refresh_rate: target.display().map(|d| d.refresh_rate()).unwrap_or(60.0),
                    })
                    .collect();

                print_list(format, &items, || {
                    for item in &items {
                        println!(
                            "window {}:\n  id: {}\n  name: {} ({})\n  fps: {}",
                            item.index, item.id, item.name, item.owner_name, item.refresh_rate
                        );
                    }
                });
            }
            Some(RecordCommands::Cameras) => {
                #[cfg(any(target_os = "macos", windows))]
                {
                    let cameras: Vec<CameraInfo> = cap_camera::list_cameras()
                        .map(|c| CameraInfo {
                            display_name: c.display_name().to_string(),
                        })
                        .collect();

                    print_list(format, &cameras, || {
                        for (i, camera) in cameras.iter().enumerate() {
                            println!("camera {}:\n  name: {}", i, camera.display_name);
                        }
                    });
                }

                #[cfg(not(any(target_os = "macos", windows)))]
                {
                    print_list(format, &Vec::<CameraInfo>::new(), || {
                        println!("Camera listing is not supported on this platform");
                    });
                }
            }
            Some(RecordCommands::Mics) => {
                let mics = cap_recording::feeds::microphone::MicrophoneFeed::list();

                let items: Vec<MicInfo> = mics
                    .keys()
                    .map(|name| MicInfo { name: name.clone() })
                    .collect();

                print_list(format, &items, || {
                    for (i, mic) in items.iter().enumerate() {
                        println!("mic {}:\n  name: {}", i, mic.name);
                    }
                });
            }
            None => {
                if let Err(e) = args.run(format).await {
                    print_error(format, &e);
                    std::process::exit(1);
                }
            }
        },
        Commands::Screenshot(s) => {
            if let Err(e) = s.run(format).await {
                print_error(format, &e);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

#[derive(Args)]
struct Export {
    project_path: PathBuf,
    output_path: Option<PathBuf>,

    #[arg(long, default_value = "60")]
    fps: u32,

    #[arg(long, value_parser = parse_resolution, default_value = "1920x1080")]
    resolution: (u32, u32),

    #[arg(long, value_enum, default_value = "maximum")]
    compression: CompressionLevel,
}

impl Export {
    async fn run(self, format: OutputFormat) -> Result<(), String> {
        status_message(&format!("Exporting '{}' ...", self.project_path.display()));

        let exporter_base = ExporterBase::builder(self.project_path)
            .build()
            .await
            .map_err(|v| format!("Exporter build error: {v}"))?;

        let frame_count = Arc::new(AtomicU32::new(0));
        let frame_count_clone = frame_count.clone();

        let exporter_output_path = cap_export::mp4::Mp4ExportSettings {
            fps: self.fps,
            resolution_base: XY::new(self.resolution.0, self.resolution.1),
            compression: self.compression.into(),
            custom_bpp: None,
            force_ffmpeg_decoder: false,
        }
        .export(exporter_base, move |f| {
            frame_count_clone.store(f, std::sync::atomic::Ordering::Relaxed);
            eprint!("\rRendered frame {f}");
            std::io::stderr().flush().ok();
            true
        })
        .await
        .map_err(|v| format!("Export error: {v}"))?;

        eprintln!();

        let output_path = if let Some(output_path) = self.output_path {
            std::fs::copy(&exporter_output_path, &output_path)
                .map_err(|e| format!("Failed to copy output: {e}"))?;
            output_path
        } else {
            exporter_output_path
        };

        let result = ExportResult {
            output_path: output_path.display().to_string(),
        };

        print_json_value(format, &result, || {
            println!("Exported video to '{}'", result.output_path);
        });

        Ok(())
    }
}
