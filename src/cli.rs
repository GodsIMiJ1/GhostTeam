use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::Command;

use crate::{agent, db, model::ghostos::GhostOsConfig, tasks};

#[derive(Debug, Parser)]
#[command(
    name = "ghostteam",
    about = "GhostTeam coordination CLI by GodsIMiJ AI Solutions Inc."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
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
    Bench,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
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
        Commands::Bench => {
            let status = Command::new("cargo").arg("bench").status()?;
            if !status.success() {
                anyhow::bail!("cargo bench failed with status {status}");
            }
        }
    }

    Ok(())
}
