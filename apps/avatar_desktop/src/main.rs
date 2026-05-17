mod app;
mod face_textures;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use winit::event_loop::{ControlFlow, EventLoop};

use crate::app::{App, StartupOptions};

fn main() -> anyhow::Result<()> {
    init_logging();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        cwd = ?std::env::current_dir().ok(),
        "starting Avatar Studio"
    );

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let options = parse_args(std::env::args().skip(1))?;
    let mut app = App::new(options)?;
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn parse_args<I, S>(args: I) -> anyhow::Result<StartupOptions>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut options = StartupOptions::default();
    let mut iter = args.into_iter().map(Into::into).peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--agent-capture" => options.mode = crate::app::StartupMode::AgentCapture,
            "--agent-perf" => options.mode = crate::app::StartupMode::AgentPerf,
            "--agent-gif" => options.mode = crate::app::StartupMode::AgentGif,
            "--show-perf" => options.show_perf = true,
            "--no-vsync" => options.vsync = false,
            "--deterministic" => options.deterministic = true,
            "--perf-frames" => {
                let Some(value) = iter.next() else {
                    anyhow::bail!("--perf-frames requires a positive integer");
                };
                options.perf_frames = parse_perf_frames(&value)?;
            }
            _ if arg.starts_with("--perf-frames=") => {
                let value = arg.trim_start_matches("--perf-frames=");
                options.perf_frames = parse_perf_frames(value)?;
            }
            _ => anyhow::bail!("unknown argument: {arg}"),
        }
    }
    Ok(options)
}

fn parse_perf_frames(value: &str) -> anyhow::Result<usize> {
    let parsed: usize = value
        .parse()
        .map_err(|_| anyhow::anyhow!("--perf-frames must be a positive integer"))?;
    if parsed == 0 {
        anyhow::bail!("--perf-frames must be greater than zero");
    }
    Ok(parsed)
}

fn init_logging() {
    // Quiet wgpu/naga; let our own crates log at info by default.
    // `wgpu_hal::vulkan=error` suppresses the Vulkan loader's per-frame
    // "Suboptimal present" warnings and Steam-overlay layer load errors,
    // which are benign on this configuration.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn,wgpu_hal::vulkan=off,naga=warn")
    });
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true))
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::StartupMode;

    #[test]
    fn parses_agent_perf_frames() {
        let opts = parse_args(["--agent-perf", "--perf-frames", "120"]).unwrap();
        assert_eq!(opts.mode, StartupMode::AgentPerf);
        assert_eq!(opts.perf_frames, 120);
    }

    #[test]
    fn parses_agent_gif_flag() {
        let opts = parse_args(["--agent-gif", "--deterministic"]).unwrap();
        assert_eq!(opts.mode, StartupMode::AgentGif);
        assert!(opts.deterministic);
    }

    #[test]
    fn parses_show_perf_flag() {
        let opts = parse_args(["--show-perf"]).unwrap();
        assert!(opts.show_perf);
    }

    #[test]
    fn rejects_invalid_perf_frames() {
        assert!(parse_args(["--agent-perf", "--perf-frames", "0"]).is_err());
        assert!(parse_args(["--agent-perf", "--perf-frames", "nope"]).is_err());
    }

    #[test]
    fn parses_no_vsync_flag() {
        let opts = parse_args(["--no-vsync"]).unwrap();
        assert!(!opts.vsync);
        let opts = parse_args(Vec::<String>::new()).unwrap();
        assert!(opts.vsync, "default is vsync on");
    }

    #[test]
    fn parses_deterministic_flag() {
        let opts = parse_args(["--deterministic"]).unwrap();
        assert!(opts.deterministic);
        let opts = parse_args(Vec::<String>::new()).unwrap();
        assert!(!opts.deterministic, "default is non-deterministic");
    }
}
