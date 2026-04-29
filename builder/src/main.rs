use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

mod aggregate;
mod appstream;
mod build_json;
mod manifest;
mod nix;
mod pipeline;
mod source;
mod wrappers;

use crate::pipeline::{Ctx, run_aggregate, run_build, run_build_all, run_list, run_regenerate};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Repo root with `registry/` and `builds/`. Defaults to cwd.
    #[arg(long, value_name = "DIR", global = true)]
    repo_root: Option<PathBuf>,

    /// AppStream catalog output dir. Defaults to `<repo>/var/appstream`.
    #[arg(long, value_name = "DIR", global = true)]
    publish_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print a JSON list of `(app, system)` pairs declared in the registry.
    List(ListArgs),
    /// Build one `(app, system)` pair, or every pair with `--all`.
    Build(BuildArgs),
    /// AppStream operations on existing build records.
    #[command(subcommand)]
    Appstream(AppstreamCommand),
}

#[derive(Debug, Subcommand)]
enum AppstreamCommand {
    /// Regenerate per-app AppStream slices from existing build records.
    Regenerate(RegenerateArgs),
    /// Combine per-app AppStream slices into one catalog for a single system.
    Aggregate(AggregateArgs),
}

#[derive(Debug, Args)]
struct ListArgs {
    /// Only emit pairs whose committed build record is missing or stale
    /// (source rev/hash, version override, attr, or mainProgram changed).
    #[arg(long)]
    outdated: bool,
}

#[derive(Debug, Args)]
#[command(group(
    clap::ArgGroup::new("scope")
        .required(true)
        .args(["all", "app"]),
))]
struct BuildArgs {
    /// Build every `(app, system)` pair in the registry.
    #[arg(long, conflicts_with = "app")]
    all: bool,

    /// AppStream component id of the app to build.
    #[arg(long, requires = "system")]
    app: Option<String>,

    /// Target system to build for.
    #[arg(long)]
    system: Option<String>,
}

#[derive(Debug, Args)]
struct RegenerateArgs {
    /// AppStream component id. If omitted, regenerate every app.
    #[arg(long)]
    app: Option<String>,

    /// Target system. If omitted, regenerate every system.
    #[arg(long)]
    system: Option<String>,
}

#[derive(Debug, Args)]
struct AggregateArgs {
    /// Target system. Slices are read from `<slices-dir>/<system>/`.
    #[arg(long)]
    system: String,

    /// Slice tree (`<system>/{xmls,icons}/...`). Defaults to `var/appstream`.
    #[arg(long, value_name = "DIR")]
    slices_dir: Option<PathBuf>,

    /// Output dir for the combined `share/swcatalog/` tree.
    #[arg(long, value_name = "DIR")]
    out_dir: PathBuf,
}

fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let repo_root = match cli.repo_root {
        Some(dir) => dir,
        None => std::env::current_dir()?,
    };

    let ctx = Ctx::new(repo_root, cli.publish_dir);

    match cli.command {
        Command::List(args) => run_list(&ctx, args.outdated),
        Command::Build(args) => {
            if args.all {
                run_build_all(&ctx, args.system.as_deref())
            } else {
                let app = args.app.expect("clap enforces --app when not --all");
                let system = args.system.expect("clap enforces --system with --app");
                run_build(&ctx, &app, &system)
            }
        }
        Command::Appstream(cmd) => match cmd {
            AppstreamCommand::Regenerate(args) => {
                run_regenerate(&ctx, args.app.as_deref(), args.system.as_deref())
            }
            AppstreamCommand::Aggregate(args) => {
                let slices_dir = args.slices_dir.unwrap_or_else(|| ctx.publish_dir.clone());
                run_aggregate(slices_dir, args.out_dir, &args.system)
            }
        },
    }
}

fn init_tracing() {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}
