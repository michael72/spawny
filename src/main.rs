use anyhow::{Context, Result};
use clap::{Arg, Command as ClapCommand};
use futures::future::try_join_all;
use std::collections::HashSet;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

// Track all running processes
type ProcessRegistry = Arc<Mutex<HashSet<u32>>>;

async fn execute_process(program: &str, args: &[String], registry: ProcessRegistry) -> Result<()> {
    println!("Executing {} with args {:?}", program, args);

    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to spawn {}", program))?;

    // Register the process ID
    let pid = child.id().expect("Failed to get process ID");
    registry.lock().await.insert(pid);

    let status = child
        .wait()
        .await
        .with_context(|| format!("Failed to wait for {}", program))?;

    // Remove process from registry after it completes
    registry.lock().await.remove(&pid);

    if !status.success() {
        kill_all_processes(&registry).await;
        anyhow::bail!("Process {} exited with: {}", program, status);
    }

    Ok(())
}

async fn kill_all_processes(registry: &ProcessRegistry) {
    let pids: Vec<u32> = registry.lock().await.iter().copied().collect();
    for pid in pids {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    registry.lock().await.clear();
}

async fn execute_sequential_chain(
    chain: &[(String, Vec<String>)],
    registry: ProcessRegistry,
) -> Result<()> {
    for (i, (program, args)) in chain.iter().enumerate() {
        if let Err(e) = execute_process(program, args, registry.clone()).await {
            kill_all_processes(&registry).await;
            return Err(e);
        }

        // If this was the last process in the chain, kill all remaining processes
        if i == chain.len() - 1 {
            println!("Chain completed successfully, terminating all processes");
            kill_all_processes(&registry).await;
            return Ok(());
        }
    }
    Ok(())
}

async fn execute_process_chains(process_chains: Vec<Vec<(String, Vec<String>)>>) -> Result<()> {
    let registry: ProcessRegistry = Arc::new(Mutex::new(HashSet::new()));

    // Convert each chain into a future that executes its processes sequentially
    let chain_futures: Vec<_> = process_chains
        .iter()
        .map(|chain| execute_sequential_chain(chain, registry.clone()))
        .collect();

    // We use try_join_all to execute all chains in parallel
    // When any chain completes (success or error), all processes will be killed
    if let Err(e) = try_join_all(chain_futures).await {
        // Error case is already handled in execute_sequential_chain
        return Err(e);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments using clap
    let matches = ClapCommand::new("spawny")
        .version("1.0")
        .about("Spawny: A tool to spawn and manage multiple programs in parallel")
        .arg(
            Arg::new("separator")
                .required(true)
                .help("Separator token (e.g. '::')"),
        )
        .arg(
            Arg::new("commands")
                .num_args(1..)
                .allow_hyphen_values(true) // This allows hyphens in values
                .trailing_var_arg(true) // This treats everything after as raw values
                .help("Programs and their arguments, separated by the separator token")
                .long_help(
                    "Programs and arguments separated by the separator
<separator> <prog1> <args1...> <separator> <prog2> <args2> <separator> ... <progN> <argsN>

The actual separator doubled means that the following command will be executed sequentially 
when the previous command finishes. The default separator :: as :::: is a sequential separator.

When a chain of commands (or the single command) executed in parallel finishes, the whole
program is finished.

Examples:
# executes hello and world (the latter with parameter --doit) in parallel
spawny -:- hello -:- world --doit
# opens both editors and exits if one of the editors is closed
spawny :: gedit :: meld
# another example: delayed execution of the client after the server started
spawny :: server --some-param --another-param=x :: sleep 2 :::: client -param

The separator could be any character except special characters (inside a shell).
See https://mywiki.wooledge.org/BashGuide/SpecialCharacters
",
                ),
        )
        .get_matches();

    let separator = matches.get_one::<String>("separator").unwrap();
    let seq_separator = [separator.clone(), separator.clone()].join("");
    let commands: Vec<String> = matches.get_many("commands").unwrap().cloned().collect();

    let process_chains: Vec<Vec<(String, Vec<String>)>> = commands
        .split(|token| token == separator)
        .filter(|group| !group.is_empty())
        .map(|group| {
            group
                .split(|token| token == &seq_separator)
                .filter(|sub_group| !sub_group.is_empty())
                .map(|sub_group| {
                    let (program, args) = sub_group.split_first().unwrap();
                    (
                        program.to_string(),
                        args.iter().map(|s| s.to_string()).collect(),
                    )
                })
                .collect()
        })
        .collect();

    if let Err(e) = execute_process_chains(process_chains).await {
        eprintln!("{:#}", e);
        std::process::exit(1);
    }

    Ok(())
}
