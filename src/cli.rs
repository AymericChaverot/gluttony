use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use crate::scanner::{Ecosystem, Filter};

const AFTER_HELP: &str = "\
Examples:
  gluttony                         scan your home directory
  gluttony ~/code --projects       per-project breakdown of ~/code
  gluttony clean --older-than 3mo  pick among projects idle for 3 months
  gluttony clean --only node --all move every node_modules to the trash
  gluttony undo                    restore a previous clean";

#[derive(Parser)]
#[command(
    name = "gluttony",
    bin_name = "gluttony",
    version,
    about = "Reclaim the disk space your toolchain ate without asking",
    after_help = AFTER_HELP,
    args_conflicts_with_subcommands = true,
    subcommand_precedence_over_arg = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub scan: ScanArgs,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scan and show what can be reclaimed (default)
    Scan(ScanArgs),
    /// Pick artefacts and move them to the trash
    Clean(CleanArgs),
    /// Restore a previous clean session
    Undo,
    /// Show what the trash holds, or empty it
    Trash {
        #[command(subcommand)]
        action: Option<TrashAction>,
    },
    /// Print shell completions
    Completions {
        #[arg(value_name = "SHELL")]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
pub enum TrashAction {
    /// Permanently delete everything in the trash
    Empty {
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Args, Clone, Default)]
pub struct FilterArgs {
    /// Directory to scan [default: home directory]
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Only these ecosystems (comma separated)
    #[arg(long, value_delimiter = ',', value_name = "ECOSYSTEMS")]
    pub only: Vec<Ecosystem>,

    /// Skip paths containing this text (repeatable)
    #[arg(long, value_name = "TEXT")]
    pub exclude: Vec<String>,

    /// Ignore artefacts smaller than this (e.g. 50MB, 1.5G)
    #[arg(long, value_name = "SIZE", value_parser = parse_size)]
    pub min_size: Option<u64>,

    /// Only projects idle for at least this long (e.g. 30d, 2w, 6mo, 1y)
    #[arg(long, value_name = "AGE", value_parser = parse_age)]
    pub older_than: Option<Duration>,
}

impl FilterArgs {
    pub fn filter(&self) -> Filter {
        Filter {
            only: self.only.clone(),
            exclude: self.exclude.clone(),
            min_size: self.min_size,
            older_than: self.older_than,
        }
    }
}

#[derive(Args, Clone, Default)]
pub struct ScanArgs {
    #[command(flatten)]
    pub filters: FilterArgs,

    /// List every artefact with its size, last activity and path
    #[arg(short, long)]
    pub list: bool,

    /// Group artefacts by project
    #[arg(short, long)]
    pub projects: bool,

    /// Machine-readable output
    #[arg(long, conflicts_with_all = ["list", "projects"])]
    pub json: bool,
}

#[derive(Args, Clone, Default)]
pub struct CleanArgs {
    #[command(flatten)]
    pub filters: FilterArgs,

    /// Take everything that matches, without the picker
    #[arg(short, long)]
    pub all: bool,

    /// Show what would be removed, touch nothing
    #[arg(long)]
    pub dry_run: bool,

    /// Delete for good instead of moving to the trash
    #[arg(long, conflicts_with = "dry_run")]
    pub permanent: bool,

    /// Do not ask for confirmation
    #[arg(short, long)]
    pub yes: bool,
}

fn split_number(s: &str) -> Result<(f64, String), String> {
    let s = s.trim();
    let idx = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(idx);
    let value: f64 = num
        .parse()
        .map_err(|_| format!("`{s}` does not start with a number"))?;
    Ok((value, unit.trim().to_ascii_lowercase()))
}

pub fn parse_size(s: &str) -> Result<u64, String> {
    let (value, unit) = split_number(s)?;
    let mult = match unit.as_str() {
        "" | "b" => 1.0,
        "k" | "kb" => 1e3,
        "m" | "mb" => 1e6,
        "g" | "gb" => 1e9,
        "t" | "tb" => 1e12,
        _ => return Err(format!("unknown size unit `{unit}` (use KB, MB, GB, TB)")),
    };
    Ok((value * mult) as u64)
}

pub fn parse_age(s: &str) -> Result<Duration, String> {
    let (value, unit) = split_number(s)?;
    let day = 86_400.0;
    let secs = match unit.as_str() {
        "h" => 3_600.0,
        "d" | "" => day,
        "w" => 7.0 * day,
        "m" | "mo" => 30.0 * day,
        "y" => 365.0 * day,
        _ => return Err(format!("unknown age unit `{unit}` (use h, d, w, mo, y)")),
    };
    Ok(Duration::from_secs_f64(value * secs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("100"), Ok(100));
        assert_eq!(parse_size("50MB"), Ok(50_000_000));
        assert_eq!(parse_size("1.5G"), Ok(1_500_000_000));
        assert_eq!(parse_size("2 kb"), Ok(2_000));
        assert!(parse_size("MB").is_err());
        assert!(parse_size("3 parsecs").is_err());
    }

    #[test]
    fn parses_ages() {
        assert_eq!(parse_age("30d"), Ok(Duration::from_secs(30 * 86_400)));
        assert_eq!(parse_age("2w"), Ok(Duration::from_secs(14 * 86_400)));
        assert_eq!(parse_age("6mo"), Ok(Duration::from_secs(180 * 86_400)));
        assert!(parse_age("soon").is_err());
    }

    #[test]
    fn bare_path_and_subcommands() {
        let cli = Cli::try_parse_from(["gluttony", "some/dir", "--list"]).unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.scan.filters.path, Some(PathBuf::from("some/dir")));
        assert!(cli.scan.list);

        let cli = Cli::try_parse_from(["gluttony", "clean", "--only", "node,rust"]).unwrap();
        match cli.command {
            Some(Command::Clean(args)) => {
                assert_eq!(args.filters.only, vec![Ecosystem::Node, Ecosystem::Rust]);
            }
            _ => panic!("expected clean"),
        }

        let cli = Cli::try_parse_from(["gluttony", "trash", "empty", "-y"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Trash {
                action: Some(TrashAction::Empty { yes: true })
            })
        ));
    }
}
