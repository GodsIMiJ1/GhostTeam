use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::{agent, db, konnect, model::ghostos::GhostOsConfig, tasks};

#[derive(Debug, Parser)]
#[command(
    name = "ghostteam",
    about = "GhostTeam coordination CLI by GodsIMiJ AI Solutions Inc."
)]
pub struct Cli {
    /// Start the HTTP API server instead of running a CLI command.
    #[arg(long)]
    pub api: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Version,
    Init,
    Join {
        id: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        backend: String,
    },
    Leave {
        id: String,
    },
    Agents,
    Send {
        from: String,
        to: String,
        message: String,
    },
    Receive {
        id: String,
        #[arg(long)]
        wait: bool,
    },
    #[command(name = "task-create")]
    TaskCreate {
        from: String,
        to: String,
        description: String,
    },
    #[command(name = "task-ack")]
    TaskAck {
        id: i64,
        worker: String,
    },
    #[command(name = "task-complete")]
    TaskComplete {
        id: i64,
        worker: String,
        result: String,
    },
    #[command(name = "task-requeue")]
    TaskRequeue {
        id: i64,
    },
    #[command(name = "task-list")]
    TaskList,
    Config,
    SetConfig {
        key: String,
        value: String,
    },
    #[command(name = "konnect-status")]
    KonnectStatus,
    #[command(name = "konnect-mappings")]
    KonnectMappings,
    #[command(name = "konnect-replay")]
    KonnectReplay {
        #[arg(long)]
        json: bool,
    },
    #[command(name = "konnect-export")]
    KonnectExport {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Bench,
    ApiDocs,
}

fn render_mapping_history(history: &[db::IdMappingHistoryRow], json: bool) -> Result<String> {
    if json {
        Ok(serde_json::to_string_pretty(history).expect("failed to serialize mapping history"))
    } else if history.is_empty() {
        Ok("No KasperKonnect mapping history recorded yet".to_string())
    } else {
        let mut output = String::new();
        for entry in history {
            output.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                entry.recorded_at.clone().unwrap_or_default(),
                entry.entity_kind,
                entry.local_id,
                entry.remote_id,
                entry.remote_source.clone().unwrap_or_default(),
                entry.action
            ));
        }
        Ok(output)
    }
}

pub fn run(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else {
        return Ok(());
    };

    match command {
        Commands::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Init => {
            db::init_workspace()?;
            println!("GhostTeam workspace initialized");
        }
        Commands::Join { id, role, backend } => {
            let agent_id = agent::join_agent(&id, &role, &backend)?;
            println!("Joined agent {agent_id}");
            agent::run_loop(&agent_id, &role, &backend)?;
        }
        Commands::Leave { id } => {
            agent::leave_agent(&id)?;
            println!("Left agent {id}");
        }
        Commands::Agents => {
            for agent in agent::list_agents()? {
                println!(
                    "{}\t{}\t{}\t{}",
                    agent.id,
                    agent.role,
                    agent.backend,
                    agent.joined_at.unwrap_or_default()
                );
            }
        }
        Commands::Send { from, to, message } => {
            agent::send_message(&from, &to, &message)?;
            println!("Message sent from {from} to {to}");
        }
        Commands::Receive { id, wait } => {
            let messages = agent::receive_messages(&id, wait)?;
            for message in messages {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    message.id,
                    message.sender,
                    message.recipient,
                    message.body,
                    message.created_at.unwrap_or_default(),
                    message.read
                );
            }
        }
        Commands::TaskCreate {
            from,
            to,
            description,
        } => {
            let id = tasks::create_task(&from, &to, &description)?;
            println!("Task created: {id}");
        }
        Commands::TaskAck { id, worker } => {
            tasks::ack_task(id, &worker)?;
            println!("Task {id} acknowledged by {worker}");
        }
        Commands::TaskComplete { id, worker, result } => {
            tasks::complete_task(id, &worker, &result)?;
            println!("Task {id} completed by {worker}");
        }
        Commands::TaskRequeue { id } => {
            tasks::requeue_task(id)?;
            println!("Task {id} requeued");
        }
        Commands::TaskList => {
            for task in tasks::list_tasks()? {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    task.id,
                    task.creator,
                    task.assignee.unwrap_or_default(),
                    task.description,
                    task.status,
                    task.result.unwrap_or_default(),
                    task.created_at.unwrap_or_default(),
                    task.updated_at.unwrap_or_default()
                );
            }
        }
        Commands::Config => {
            let config = GhostOsConfig::load()?;
            print!(
                "{}",
                serde_yaml::to_string(&config).expect("failed to format GhostOS config")
            );
        }
        Commands::SetConfig { key, value } => {
            let mut config = GhostOsConfig::load()?;
            match key.as_str() {
                "ghostos_endpoint" => config.ghostos_endpoint = value,
                "ghostos_model" => config.ghostos_model = value,
                other => {
                    anyhow::bail!(
                        "unknown config key: {} (supported: ghostos_endpoint, ghostos_model)",
                        other
                    );
                }
            }

            config.save()?;
            println!("Updated {key}");
        }
        Commands::KonnectStatus => {
            let registered_ids = agent::list_agents()?
                .into_iter()
                .map(|agent| agent.id)
                .collect::<Vec<_>>();

            match konnect::runtime_status(&registered_ids) {
                Some(status) if status.reachable => {
                    if let Some(health) = status.health {
                        println!(
                            "KasperKonnect: reachable\tservice={}\tversion={}\tbind={}",
                            health.service, health.version, health.bind
                        );
                    } else {
                        println!("KasperKonnect: reachable");
                    }

                    println!("Base URL: {}", status.base_url);
                    println!("Daemon environments: {}", status.environments.len());
                    println!("GhostTeam registrations: {}", status.registered.len());

                    if status.registered.is_empty() {
                        println!("GhostTeam registration visibility: none found");
                    } else {
                        println!("GhostTeam registration visibility:");
                        for environment in status.registered {
                            println!(
                                "{}\t{}\t{}\t{}",
                                environment.id,
                                environment.display_name,
                                environment.kind,
                                environment.status
                            );
                        }
                    }
                }
                Some(status) => {
                    println!("KasperKonnect: unreachable\tbase_url={}", status.base_url);
                    println!(
                        "GhostTeam registration visibility: unavailable while daemon is offline"
                    );
                }
                None => {
                    println!("KasperKonnect: not configured");
                    println!(
                        "Set GHOSTTEAM_KASPERKONNECT_URL or start the daemon locally to enable mirroring"
                    );
                }
            }
        }
        Commands::KonnectMappings => {
            let mappings = db::list_id_mappings()?;
            if mappings.is_empty() {
                println!("No KasperKonnect ID mappings recorded yet");
            } else {
                for mapping in mappings {
                    println!(
                        "{}\t{}\t{}\t{}\t{}\t{}",
                        mapping.entity_kind,
                        mapping.local_id,
                        mapping.remote_id,
                        mapping.remote_source.unwrap_or_default(),
                        mapping.created_at.unwrap_or_default(),
                        mapping.updated_at.unwrap_or_default()
                    );
                }
            }
        }
        Commands::KonnectReplay { json } => {
            let history = db::list_id_mapping_history()?;
            print!("{}", render_mapping_history(&history, json)?);
        }
        Commands::KonnectExport { json, output } => {
            let history = db::list_id_mapping_history()?;
            let rendered = render_mapping_history(&history, json)?;

            if let Some(path) = output {
                fs::write(&path, rendered.as_bytes())?;
                println!("Exported KasperKonnect mapping history to {}", path.display());
            } else {
                print!("{}", rendered);
            }
        }
        Commands::Bench => {
            let status = Command::new("cargo").arg("bench").status()?;
            if !status.success() {
                anyhow::bail!("cargo bench failed with status {status}");
            }
        }
        Commands::ApiDocs => {
            let path = env::current_dir()?.join("openapi.yaml");
            println!("{}", path.display());
        }
    }

    Ok(())
}
