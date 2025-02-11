mod format;
mod locale;
mod pty;
mod recorder;
use anyhow::Result;
use clap::{Parser, Subcommand};
use format::{asciicast, raw};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::{CString, OsString};
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

#[derive(Debug, Parser)]
#[clap(author, version, about)]
#[command(name = "asciinema")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Record terminal session
    #[command(name = "rec")]
    Record {
        filename: String,

        /// Enable input recording
        #[arg(long)]
        stdin: bool,

        /// Append to existing asciicast file
        #[arg(long)]
        append: bool,

        /// Save raw output only
        #[arg(long)]
        raw: bool,

        /// Overwrite target file if it already exists
        #[arg(long, conflicts_with = "append")]
        overwrite: bool,

        /// Command to record [default: $SHELL]
        #[arg(short, long)]
        command: Option<String>,

        /// List of env vars to save
        #[arg(short, long, default_value_t = String::from("SHELL,TERM"))]
        env: String,

        /// Title of the recording
        #[arg(short, long)]
        title: Option<String>,

        /// Limit idle time to given number of seconds
        #[arg(short, long, value_name = "SECS")]
        idle_time_limit: Option<f32>,

        /// Override terminal width (columns) for recorded command
        #[arg(long)]
        cols: Option<u16>,

        /// Override terminal height (rows) for recorded command
        #[arg(long)]
        rows: Option<u16>,

        /// Quiet mode - suppress all notices/warnings
        #[arg(short, long)]
        quiet: bool,
    },

    /// Play terminal session
    Play {
        filename: String,

        /// Limit idle time to given number of seconds
        #[arg(short, long, value_name = "SECS")]
        idle_time_limit: Option<f64>,

        /// Set playback speed
        #[arg(short, long)]
        speed: Option<f64>,

        /// Loop loop loop loop
        #[arg(short, long, name = "loop")]
        loop_: bool,

        /// Automatically pause on markers
        #[arg(short = 'm', long)]
        pause_on_markers: bool,
    },

    /// Print full output of terminal sessions
    Cat {
        #[arg(required = true)]
        filename: Vec<String>,
    },

    /// Upload recording to asciinema.org
    Upload {
        /// Filename/path of asciicast to upload
        filename: String,
    },

    /// Link this system to asciinema.org account
    Auth,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Record {
            filename,
            stdin,
            mut append,
            raw,
            mut overwrite,
            command,
            env,
            title,
            idle_time_limit,
            cols,
            rows,
            quiet,
        } => {
            locale::check_utf8_locale()?;

            let path = Path::new(&filename);

            if path.exists() {
                let metadata = fs::metadata(path)?;

                if metadata.len() == 0 {
                    overwrite = true;
                    append = false;
                }
                // TODO if !append && !overwrite - error message
            } else {
                append = false;
            }

            let file = fs::OpenOptions::new()
                .write(true)
                .append(append)
                .create(overwrite)
                .create_new(!overwrite && !append)
                .truncate(overwrite)
                .open(&filename)?;

            let writer: Box<dyn format::Writer + Send> = if raw {
                Box::new(raw::Writer::new(file))
            } else {
                let time_offset = if append {
                    asciicast::get_duration(&filename)?
                } else {
                    0.0
                };

                Box::new(asciicast::Writer::new(file, time_offset))
            };

            let mut recorder = recorder::Recorder::new(
                writer,
                append,
                stdin,
                idle_time_limit,
                command.clone(),
                title,
                capture_env(&env),
            );

            let exec_args = build_exec_args(command);
            let exec_env = build_exec_env();

            pty::exec(&exec_args, &exec_env, (cols, rows), &mut recorder)?;
        }

        Commands::Play {
            filename,
            idle_time_limit,
            speed,
            loop_,
            pause_on_markers,
        } => todo!(),

        Commands::Cat { filename } => todo!(),

        Commands::Upload { filename } => todo!(),

        Commands::Auth => todo!(),
    }

    Ok(())
}

fn capture_env(vars: &str) -> HashMap<String, String> {
    let vars = vars.split(',').collect::<HashSet<_>>();

    env::vars()
        .filter(|(k, _v)| vars.contains(&k.as_str()))
        .collect::<HashMap<_, _>>()
}

fn build_exec_args(command: Option<String>) -> Vec<String> {
    let command = command
        .or(env::var("SHELL").ok())
        .unwrap_or("/bin/sh".to_owned());

    vec!["/bin/sh".to_owned(), "-c".to_owned(), command]
}

fn build_exec_env() -> Vec<CString> {
    env::vars_os()
        .map(format_env_var)
        .chain(std::iter::once(CString::new("ASCIINEMA_REC=1").unwrap()))
        .collect()
}

fn format_env_var((key, value): (OsString, OsString)) -> CString {
    let mut key_value = key.into_vec();
    key_value.push(b'=');
    key_value.extend(value.into_vec());

    CString::new(key_value).unwrap()
}
