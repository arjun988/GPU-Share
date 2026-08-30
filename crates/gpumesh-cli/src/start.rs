//! Interactive `gpumesh start` menu — Claude CMD-style terminal UI.

use anyhow::Result;
use clap::CommandFactory;
use console::{style, Style, Term};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use owo_colors::OwoColorize;

use crate::commands;
use crate::group::GroupCmd;
use crate::ui;
use crate::{Cli, Commands, ConfigAction};

#[derive(Clone, Copy)]
enum Item {
    Header(&'static str),
    Action(Action),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Share,
    SharePublic,
    PairCode,
    Pair,
    Peers,
    RunPeer,
    DesktopShare,
    DesktopConnect,
    DesktopAllow,
    DesktopDoctor,
    AppSync,
    AppRun,
    AppPull,
    CudaShare,
    CudaAllow,
    CudaDemo,
    CudaDoctor,
    Status,
    Gpu,
    Doctor,
    GroupCreate,
    GroupList,
    GroupInvite,
    GroupJoin,
    GroupAdd,
    GroupMembers,
    RunGroup,
    Search,
    Jobs,
    Sync,
    Dashboard,
    ConfigShow,
    Init,
    Help,
    Exit,
}

const MENU: &[Item] = &[
    Item::Header("Core"),
    Item::Action(Action::Share),
    Item::Action(Action::SharePublic),
    Item::Action(Action::PairCode),
    Item::Action(Action::Pair),
    Item::Action(Action::Peers),
    Item::Action(Action::RunPeer),
    Item::Header("Desktop"),
    Item::Action(Action::DesktopShare),
    Item::Action(Action::DesktopConnect),
    Item::Action(Action::DesktopAllow),
    Item::Action(Action::DesktopDoctor),
    Item::Header("Apps (hybrid)"),
    Item::Action(Action::AppSync),
    Item::Action(Action::AppRun),
    Item::Action(Action::AppPull),
    Item::Header("CUDA remoting (R2)"),
    Item::Action(Action::CudaShare),
    Item::Action(Action::CudaAllow),
    Item::Action(Action::CudaDemo),
    Item::Action(Action::CudaDoctor),
    Item::Action(Action::Status),
    Item::Action(Action::Gpu),
    Item::Header("Cluster"),
    Item::Action(Action::GroupCreate),
    Item::Action(Action::GroupList),
    Item::Action(Action::GroupInvite),
    Item::Action(Action::GroupJoin),
    Item::Action(Action::GroupAdd),
    Item::Action(Action::GroupMembers),
    Item::Action(Action::RunGroup),
    Item::Header("Tools"),
    Item::Action(Action::Doctor),
    Item::Action(Action::Search),
    Item::Action(Action::Jobs),
    Item::Action(Action::Sync),
    Item::Action(Action::Dashboard),
    Item::Action(Action::ConfigShow),
    Item::Action(Action::Init),
    Item::Header("Help"),
    Item::Action(Action::Help),
    Item::Action(Action::Exit),
];

fn theme() -> ColorfulTheme {
    ColorfulTheme {
        defaults_style: Style::new().for_stderr().cyan(),
        prompt_style: Style::new().for_stderr().bold(),
        prompt_prefix: style("?".to_string()).for_stderr().yellow().bright(),
        prompt_suffix: style("›".to_string()).for_stderr().black().bright(),
        success_prefix: style("✔".to_string()).for_stderr().green(),
        success_suffix: style("·".to_string()).for_stderr().black().bright(),
        error_prefix: style("✖".to_string()).for_stderr().red(),
        error_style: Style::new().for_stderr().red(),
        hint_style: Style::new().for_stderr().black().bright(),
        values_style: Style::new().for_stderr().cyan(),
        active_item_style: Style::new().for_stderr().cyan().bold(),
        inactive_item_style: Style::new().for_stderr(),
        active_item_prefix: style("❯".to_string()).for_stderr().cyan().bold(),
        inactive_item_prefix: style(" ".to_string()).for_stderr(),
        checked_item_prefix: style("✔".to_string()).for_stderr().green(),
        unchecked_item_prefix: style("○".to_string()).for_stderr().black().bright(),
        picked_item_prefix: style("❯".to_string()).for_stderr().cyan(),
        unpicked_item_prefix: style(" ".to_string()).for_stderr(),
    }
}

fn label(action: Action) -> &'static str {
    match action {
        Action::Share => "Share GPU (private)",
        Action::SharePublic => "Share GPU (public listing)",
        Action::PairCode => "Show pairing code",
        Action::Pair => "Pair with a peer",
        Action::Peers => "List peers",
        Action::RunPeer => "Run job on a peer",
        Action::DesktopShare => "Share GPU desktop (RDP/VNC)",
        Action::DesktopConnect => "Connect to peer desktop",
        Action::DesktopAllow => "Allow peer for desktop",
        Action::DesktopDoctor => "Desktop backend doctor",
        Action::AppSync => "Sync project to a peer",
        Action::AppRun => "Run project on a peer GPU",
        Action::AppPull => "Pull job outputs from a peer",
        Action::CudaShare => "Share GPU for CUDA remoting",
        Action::CudaAllow => "Allow peer for CUDA remoting",
        Action::CudaDemo => "Run CUDA remoting demo",
        Action::CudaDoctor => "CUDA remoting doctor",
        Action::Status => "Node status",
        Action::Gpu => "GPU inventory",
        Action::Doctor => "Doctor (diagnose setup)",
        Action::GroupCreate => "Create group",
        Action::GroupList => "List groups",
        Action::GroupInvite => "Invite to group",
        Action::GroupJoin => "Join group via invite",
        Action::GroupAdd => "Add peer to group",
        Action::GroupMembers => "List group members",
        Action::RunGroup => "Run job on group (scheduler)",
        Action::Search => "Search public GPUs",
        Action::Jobs => "Recent jobs",
        Action::Sync => "Sync to dashboard API",
        Action::Dashboard => "Dashboard URLs",
        Action::ConfigShow => "Show config",
        Action::Init => "Initialize node",
        Action::Help => "CLI help",
        Action::Exit => "Exit",
    }
}

fn menu_labels() -> Vec<String> {
    MENU.iter()
        .map(|item| match item {
            Item::Header(h) => format!("  --- {h} ---"),
            Item::Action(a) => format!("  {}", label(*a)),
        })
        .collect()
}

fn print_logo() {
    let logo = r#"
   ██████╗ ██████╗ ██╗   ██╗███╗   ███╗███████╗███████╗██╗  ██╗
  ██╔════╝ ██╔══██╗██║   ██║████╗ ████║██╔════╝██╔════╝██║  ██║
  ██║  ███╗██████╔╝██║   ██║██╔████╔██║█████╗  ███████╗███████║
  ██║   ██║██╔═══╝ ██║   ██║██║╚██╔╝██║██╔══╝  ╚════██║██╔══██║
  ╚██████╔╝██║     ╚██████╔╝██║ ╚═╝ ██║███████╗███████║██║  ██║
   ╚═════╝ ╚═╝      ╚═════╝ ╚═╝     ╚═╝╚══════╝╚══════╝╚═╝  ╚═╝
"#;
    // Coral / reddish-orange like Claude CMD
    print!("{}", logo.truecolor(232, 112, 72).bold());
    println!(
        "  {}  {}",
        format!("GPUMesh v{}", gpumesh_common::VERSION)
            .truecolor(232, 112, 72)
            .bold(),
        "P2P GPU sharing".bright_black()
    );
    println!();
}

fn prompt_string(msg: &str) -> Result<String> {
    let theme = theme();
    let val: String = Input::with_theme(&theme)
        .with_prompt(msg)
        .allow_empty(false)
        .interact_text()?;
    Ok(val)
}

fn prompt_string_default(msg: &str, default: &str) -> Result<String> {
    let theme = theme();
    let val: String = Input::with_theme(&theme)
        .with_prompt(msg)
        .default(default.to_string())
        .allow_empty(false)
        .interact_text()?;
    Ok(val)
}

fn pause_enter() {
    let _ = Confirm::with_theme(&theme())
        .with_prompt("Back to menu")
        .default(true)
        .show_default(false)
        .interact();
}

pub async fn run() -> Result<()> {
    let term = Term::stdout();
    loop {
        let _ = term.clear_screen();
        print_logo();

        let labels = menu_labels();
        // Skip headers: start on first real action
        let mut start = 1usize;
        while start < MENU.len() {
            if matches!(MENU[start], Item::Action(_)) {
                break;
            }
            start += 1;
        }

        let selection = Select::with_theme(&theme())
            .with_prompt("What would you like to do? (Use arrow keys)")
            .items(&labels)
            .default(start)
            .interact_opt()?;

        let Some(idx) = selection else {
            ui::dim("Bye.");
            break;
        };

        match MENU.get(idx) {
            Some(Item::Header(_)) => continue,
            Some(Item::Action(Action::Exit)) => {
                ui::ok("Goodbye.");
                break;
            }
            Some(Item::Action(action)) => {
                println!();
                if let Err(e) = dispatch_action(*action).await {
                    ui::err(e.to_string());
                }
                println!();
                pause_enter();
            }
            None => break,
        }
    }
    Ok(())
}

async fn dispatch_action(action: Action) -> Result<()> {
    match action {
        Action::Share => {
            ui::info("Starting share (Ctrl+C to stop)…");
            commands::dispatch(Commands::Share {
                max_vram: None,
                max_gpu_utilization: None,
                public: false,
                region: None,
                action: None,
            })
            .await
        }
        Action::SharePublic => {
            let region = prompt_string_default("Region (e.g. us-west, or empty)", "")?;
            ui::info("Starting public share (Ctrl+C to stop)…");
            commands::dispatch(Commands::Share {
                max_vram: None,
                max_gpu_utilization: None,
                public: true,
                region: if region.is_empty() {
                    None
                } else {
                    Some(region)
                },
                action: None,
            })
            .await
        }
        Action::PairCode => commands::dispatch(Commands::PairCode).await,
        Action::Pair => {
            let code = prompt_string("Paste peer pairing code")?;
            commands::dispatch(Commands::Pair { code }).await
        }
        Action::Peers => commands::dispatch(Commands::Peers).await,
        Action::RunPeer => {
            let peer = prompt_string("Peer name")?;
            let image = prompt_string_default("Container image", "python:3.12-slim")?;
            let cmdline = prompt_string_default("Command", "nvidia-smi")?;
            let command = shell_words(&cmdline);
            commands::dispatch(Commands::Run {
                peer: Some(peer),
                group: None,
                gpu_memory: None,
                image: Some(image),
                env: vec![],
                workdir: ".".into(),
                file: None,
                retries: 0,
                command,
            })
            .await
        }
        Action::DesktopShare => {
            crate::desktop::dispatch(crate::desktop::DesktopCmd::Share).await
        }
        Action::DesktopConnect => {
            let peer = prompt_string("Peer name")?;
            crate::desktop::dispatch(crate::desktop::DesktopCmd::Connect { peer, port: None })
                .await
        }
        Action::DesktopAllow => {
            let peer = prompt_string("Peer name")?;
            crate::desktop::dispatch(crate::desktop::DesktopCmd::Allow { peer }).await
        }
        Action::DesktopDoctor => {
            crate::desktop::dispatch(crate::desktop::DesktopCmd::Doctor).await
        }
        Action::AppSync => {
            let peer = prompt_string("Peer name")?;
            let dir = prompt_string_default("Project directory", ".")?;
            commands::dispatch(Commands::App {
                action: crate::app::AppCmd::Sync {
                    peer,
                    dir,
                    name: None,
                },
            })
            .await
        }
        Action::AppRun => {
            let peer = prompt_string("Peer name")?;
            let dir = prompt_string_default("Project directory", ".")?;
            let cmdline = prompt_string_default("Command", "python train.py")?;
            let command = shell_words(&cmdline);
            let desktop = Confirm::with_theme(&theme())
                .with_prompt("Also print GPU desktop connect hint?")
                .default(false)
                .interact()?;
            commands::dispatch(Commands::App {
                action: crate::app::AppCmd::Run {
                    peer: Some(peer),
                    group: None,
                    gpu_memory: None,
                    image: None,
                    env: vec![],
                    dir,
                    out: None,
                    desktop,
                    command,
                },
            })
            .await
        }
        Action::AppPull => {
            let peer = prompt_string("Peer name")?;
            let job = prompt_string_default("Job id (empty = latest for peer)", "")?;
            let dir = prompt_string_default("Output directory", "./gpumesh-out")?;
            commands::dispatch(Commands::App {
                action: crate::app::AppCmd::Pull {
                    peer,
                    job: if job.is_empty() { None } else { Some(job) },
                    remote: None,
                    dir,
                },
            })
            .await
        }
        Action::CudaShare => crate::cuda::dispatch(crate::cuda::CudaCmd::Share).await,
        Action::CudaAllow => {
            let peer = prompt_string("Peer name")?;
            crate::cuda::dispatch(crate::cuda::CudaCmd::Allow { peer }).await
        }
        Action::CudaDemo => {
            let peer = prompt_string("Peer name")?;
            crate::cuda::dispatch(crate::cuda::CudaCmd::Demo {
                peer,
                n: 262_144,
            })
            .await
        }
        Action::CudaDoctor => crate::cuda::dispatch(crate::cuda::CudaCmd::Doctor).await,
        Action::Status => commands::dispatch(Commands::Status).await,
        Action::Gpu => commands::dispatch(Commands::Gpu).await,
        Action::Doctor => commands::dispatch(Commands::Doctor).await,
        Action::GroupCreate => {
            let name = prompt_string_default("Group name", "research")?;
            commands::dispatch(Commands::Group {
                action: GroupCmd::Create { name },
            })
            .await
        }
        Action::GroupList => {
            commands::dispatch(Commands::Group {
                action: GroupCmd::List,
            })
            .await
        }
        Action::GroupInvite => {
            let name = prompt_string_default("Group name", "research")?;
            commands::dispatch(Commands::Group {
                action: GroupCmd::Invite { name },
            })
            .await
        }
        Action::GroupJoin => {
            let code = prompt_string("Paste group invite code")?;
            commands::dispatch(Commands::Group {
                action: GroupCmd::Join { code },
            })
            .await
        }
        Action::GroupAdd => {
            let name = prompt_string_default("Group name", "research")?;
            let peer = prompt_string("Peer name")?;
            commands::dispatch(Commands::Group {
                action: GroupCmd::Add { group: name, peer },
            })
            .await
        }
        Action::GroupMembers => {
            let name = prompt_string_default("Group name", "research")?;
            commands::dispatch(Commands::Group {
                action: GroupCmd::Members { name },
            })
            .await
        }
        Action::RunGroup => {
            let group = prompt_string_default("Group name", "research")?;
            let gpu_memory = prompt_string_default("Min GPU memory (e.g. 1GB)", "1GB")?;
            let image = prompt_string_default("Container image", "python:3.12-slim")?;
            let cmdline = prompt_string_default("Command", "echo hello")?;
            let command = shell_words(&cmdline);
            commands::dispatch(Commands::Run {
                peer: None,
                group: Some(group),
                gpu_memory: Some(gpu_memory),
                image: Some(image),
                env: vec![],
                workdir: ".".into(),
                file: None,
                retries: 0,
                command,
            })
            .await
        }
        Action::Jobs => commands::dispatch(Commands::Jobs { limit: 20 }).await,
        Action::Search => {
            let gpu = prompt_string_default("GPU filter (e.g. 4090, empty=all)", "")?;
            commands::dispatch(Commands::Search {
                gpu: if gpu.is_empty() { None } else { Some(gpu) },
                vram: None,
                cuda: None,
                region: None,
                idle: false,
                json: false,
            })
            .await
        }
        Action::Sync => commands::dispatch(Commands::Sync).await,
        Action::Dashboard => commands::dispatch(Commands::Dashboard).await,
        Action::ConfigShow => {
            commands::dispatch(Commands::Config {
                action: Some(ConfigAction::Show),
            })
            .await
        }
        Action::Init => {
            let name = prompt_string_default("Node name", "my-pc")?;
            commands::dispatch(Commands::Init { name: Some(name) }).await
        }
        Action::Help => {
            ui::print_banner();
            println!("{}", Cli::command().render_long_help());
            Ok(())
        }
        Action::Exit => Ok(()),
    }
}

fn shell_words(s: &str) -> Vec<String> {
    s.split_whitespace().map(|w| w.to_string()).collect()
}
