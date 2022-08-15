use std::env;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

use anyhow::{bail, Result};
use clap::Subcommand;
use console::style;
use derive_new::new;

use crate::framework::{Context, Module};
use crate::support::command::run_command;
use crate::support::gas::Gas;

use super::{args::BaseTxArgs, config::WasmConfig, proposal::entrypoint::ProposalCmd};
use super::{ops, proposal};

#[derive(clap::ArgEnum, Clone, Debug)]
pub enum NodePackageManager {
    Npm,
    Yarn,
}

impl FromStr for NodePackageManager {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "npm" => anyhow::Ok(Self::Npm),
            "yarn" => anyhow::Ok(Self::Yarn),
            _ => Err(anyhow::anyhow!("must be either `npm` or `yarn`")),
        }
    }
}

impl From<&NodePackageManager> for String {
    fn from(n: &NodePackageManager) -> Self {
        match n {
            NodePackageManager::Npm => "npm".to_string(),
            NodePackageManager::Yarn => "yarn".to_string(),
        }
    }
}

impl std::fmt::Display for NodePackageManager {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(String::from(self).as_str())
    }
}

#[derive(Subcommand, Debug)]
pub enum WasmCmd {
    /// Create new CosmWasm contract from boilerplate
    New {
        /// Contract name
        contract_name: String,
        /// Path to store generated contract
        #[clap(short, long)]
        target_dir: Option<PathBuf>,
        /// Template's version, using main branch if not specified
        #[clap(short, long)]
        version: Option<String>,
    },
    /// Build .wasm for storing contract code on the blockchain
    Build {
        /// If set, the contract(s) will not be optimized by wasm-opt after build (only use in dev)
        #[clap(long)]
        no_wasm_opt: bool,
        /// Option for m1 user for wasm optimization, FOR TESTING ONLY, PRODUCTION BUILD SHOULD USE INTEL BUILD
        #[clap(short, long)]
        aarch64: bool,
    },
    /// Store .wasm on chain for later initialization
    StoreCode {
        /// Name of the contract to store
        contract_name: String,

        /// If set, use non wasm-opt optimized wasm to store code (only use in dev)
        #[clap(long)]
        no_wasm_opt: bool,

        /// Restricting the code to be able to instantiate only by given address, no restriction by default
        #[clap(long)]
        permit_instantiate_only: Option<String>,

        #[clap(flatten)]
        base_tx_args: BaseTxArgs,
    },
    TsGen {
        /// Name of the contract to store
        contract_name: String,

        /// Sschema generation command, default: `cargo run -p {contract_name} --example schema`
        #[clap(long)]
        schema_gen_cmd: Option<String>,

        /// Directory of input schema for ts generation
        #[clap(long)]
        schema_dir: Option<PathBuf>,

        /// Code output directory, ignore remaining ts build process if custom out_dir is specified
        #[clap(long)]
        out_dir: Option<PathBuf>,

        /// Code output directory
        #[clap(long, default_value = "yarn")]
        node_package_manager: NodePackageManager,
    },
    /// Update admin that can migrate contract
    UpdateAdmin {
        /// Name of the contract to store
        contract_name: String,

        /// Label for the instantiated contract for later reference
        #[clap(short, long, default_value = "default")]
        label: String,

        /// Address of new admin
        #[clap(long)]
        new_admin: String,

        #[clap(flatten)]
        base_tx_args: BaseTxArgs,
    },
    /// Clear admin so no one can migrate contract
    ClearAdmin {
        /// Name of the contract to store
        contract_name: String,

        /// Label for the instantiated contract for later reference
        #[clap(short, long, default_value = "default")]
        label: String,

        #[clap(flatten)]
        base_tx_args: BaseTxArgs,
    },

    /// Instanitate .wasm stored on chain
    Instantiate {
        /// Name of the contract to instantiate
        contract_name: String,
        /// Label for the instantiated contract for later reference
        #[clap(short, long, default_value = "default")]
        label: String,

        /// Raw json string to use as instantiate msg
        #[clap(short, long)]
        raw: Option<String>,

        /// Specifying admin required for contract migration.
        /// Use "signer" for setting tx signer as admin.
        /// Use bech32 address (eg. "osmo1cyyzpxplxdzkeea7kwsydadg87357qnahakaks") for custom admin.
        #[clap(long)]
        admin: Option<String>,

        /// Funds to send to instantiated contract
        #[clap(short, long)]
        funds: Option<String>,

        /// Skip the check for proposal's updated code_id
        #[clap(long)]
        no_proposal_sync: bool,

        /// Agree to all prompts
        #[clap(short, long)]
        yes: bool,

        #[clap(flatten)]
        base_tx_args: BaseTxArgs,
    },
    /// Migrated instanitate contract to use other code stored on chain
    Migrate {
        /// Name of the contract to instantiate
        contract_name: String,
        /// Label for the instantiated contract for selcting migration target
        #[clap(short, long, default_value = "default")]
        label: String,

        /// Raw json string to use as instantiate msg
        #[clap(short, long)]
        raw: Option<String>,

        /// Skip the check for proposal's updated code_id
        #[clap(long)]
        no_proposal_sync: bool,

        /// Agree to all prompts
        #[clap(short, long)]
        yes: bool,

        #[clap(flatten)]
        base_tx_args: BaseTxArgs,
    },
    /// Build, Optimize, Store code, and instantiate contract
    Deploy {
        /// Name of the contract to deploy
        contract_name: String,

        /// Label for the instantiated contract for later reference
        #[clap(short, long, default_value = "default")]
        label: String,

        /// Raw json string to use as instantiate msg
        #[clap(short, long)]
        raw: Option<String>,

        /// Restricting the code to be able to instantiate only by given address, no restriction by default
        #[clap(long)]
        permit_instantiate_only: Option<String>,

        /// Specifying admin required for contract migration.
        /// Use "signer" for setting tx signer as admin.
        /// Use bech32 address (eg. "osmo1cyyzpxplxdzkeea7kwsydadg87357qnahakaks") for custom admin.
        #[clap(long)]
        admin: Option<String>,

        /// Funds to send to instantiated contract
        #[clap(short, long)]
        funds: Option<String>,

        /// Use existing .wasm file to deploy if set to true
        #[clap(long)]
        no_rebuild: bool,

        /// If set, skip wasm-opt and store the unoptimized code (only use in dev)
        #[clap(long)]
        no_wasm_opt: bool,

        #[clap(flatten)]
        base_tx_args: BaseTxArgs,
    },
    /// Build, Optimize, Store code, and migrate contract
    Upgrade {
        /// Name of the contract to deploy
        contract_name: String,

        /// Label for the instantiated contract for later reference
        #[clap(short, long, default_value = "default")]
        label: String,

        /// Raw json string to use as instantiate msg
        #[clap(short, long)]
        raw: Option<String>,

        /// Use existing .wasm file to deploy if set to true
        #[clap(long)]
        no_rebuild: bool,

        /// If set, skip wasm-opt and store the unoptimized code (only use in dev)
        #[clap(long)]
        no_wasm_opt: bool,

        /// Restricting the code to be able to instantiate only by given address, no restriction by default
        #[clap(long)]
        permit_instantiate_only: Option<String>,

        #[clap(flatten)]
        base_tx_args: BaseTxArgs,
    },
    Proposal {
        #[clap(subcommand)]
        cmd: ProposalCmd,
    },
}

#[derive(new)]
pub struct WasmModule {}

impl<'a> Module<'a, WasmConfig, WasmCmd, anyhow::Error> for WasmModule {
    fn execute<Ctx: Context<'a, WasmConfig>>(ctx: Ctx, cmd: &WasmCmd) -> Result<(), anyhow::Error> {
        match cmd {
            WasmCmd::New {
                contract_name: name,
                target_dir, // TODO: Rremove this
                version,
            } => ops::new(&ctx, name, version.to_owned(), target_dir.to_owned()),
            WasmCmd::Build {
                no_wasm_opt,
                aarch64,
            } => ops::build(&ctx, no_wasm_opt, aarch64),
            WasmCmd::StoreCode {
                contract_name,
                no_wasm_opt,
                permit_instantiate_only,
                base_tx_args,
            } => {
                let BaseTxArgs {
                    network,
                    signer_args,
                    gas_args,
                    timeout_height,
                }: &BaseTxArgs = base_tx_args;

                ops::store_code(
                    &ctx,
                    contract_name,
                    network,
                    no_wasm_opt,
                    permit_instantiate_only,
                    {
                        let global_conf = ctx.global_config()?;
                        &Gas::from_args(
                            gas_args,
                            global_conf.gas_price(),
                            global_conf.gas_adjustment(),
                        )?
                    },
                    timeout_height,
                    signer_args.private_key(&ctx.global_config()?)?,
                )?;
                Ok(())
            }
            WasmCmd::UpdateAdmin {
                contract_name,
                label,
                new_admin,
                base_tx_args,
            } => {
                let BaseTxArgs {
                    network,
                    signer_args,
                    gas_args,
                    timeout_height,
                }: &BaseTxArgs = base_tx_args;

                ops::update_admin(
                    &ctx,
                    contract_name,
                    label,
                    network,
                    new_admin,
                    {
                        let global_conf = ctx.global_config()?;
                        &Gas::from_args(
                            gas_args,
                            global_conf.gas_price(),
                            global_conf.gas_adjustment(),
                        )?
                    },
                    timeout_height,
                    signer_args.private_key(&ctx.global_config()?)?,
                )?;
                Ok(())
            }
            WasmCmd::ClearAdmin {
                contract_name,
                label,
                base_tx_args,
            } => {
                let BaseTxArgs {
                    network,
                    signer_args,
                    gas_args,
                    timeout_height,
                }: &BaseTxArgs = base_tx_args;

                ops::clear_admin(
                    &ctx,
                    contract_name,
                    label,
                    network,
                    {
                        let global_conf = ctx.global_config()?;
                        &Gas::from_args(
                            gas_args,
                            global_conf.gas_price(),
                            global_conf.gas_adjustment(),
                        )?
                    },
                    timeout_height,
                    signer_args.private_key(&ctx.global_config()?)?,
                )?;
                Ok(())
            }

            WasmCmd::Instantiate {
                contract_name,
                label,
                raw,
                admin,
                no_proposal_sync,
                yes,
                funds,
                base_tx_args,
            } => {
                let BaseTxArgs {
                    network,
                    signer_args,
                    gas_args,
                    timeout_height,
                }: &BaseTxArgs = base_tx_args;
                ops::instantiate(
                    &ctx,
                    contract_name,
                    label.as_str(),
                    raw.as_ref(),
                    admin.as_ref(),
                    *no_proposal_sync,
                    *yes,
                    funds.as_ref().map(|s| s.as_str()).try_into()?,
                    network,
                    timeout_height,
                    {
                        let global_conf = ctx.global_config()?;
                        &Gas::from_args(
                            gas_args,
                            global_conf.gas_price(),
                            global_conf.gas_adjustment(),
                        )?
                    },
                    signer_args.private_key(&ctx.global_config()?)?,
                )?;
                Ok(())
            }
            WasmCmd::Migrate {
                contract_name,
                label,
                raw,
                no_proposal_sync,
                yes,
                base_tx_args,
            } => {
                let BaseTxArgs {
                    network,
                    signer_args,
                    gas_args,
                    timeout_height,
                }: &BaseTxArgs = base_tx_args;
                ops::migrate(
                    &ctx,
                    contract_name,
                    label.as_str(),
                    raw.as_ref(),
                    *no_proposal_sync,
                    *yes,
                    network,
                    timeout_height,
                    {
                        let global_conf = ctx.global_config()?;
                        &Gas::from_args(
                            gas_args,
                            global_conf.gas_price(),
                            global_conf.gas_adjustment(),
                        )?
                    },
                    signer_args.private_key(&ctx.global_config()?)?,
                )?;
                Ok(())
            }
            WasmCmd::Deploy {
                contract_name,
                label,
                raw,
                permit_instantiate_only,
                admin,
                funds,
                no_rebuild,
                no_wasm_opt,
                base_tx_args,
            } => {
                let BaseTxArgs {
                    network,
                    signer_args,
                    gas_args,
                    timeout_height,
                }: &BaseTxArgs = base_tx_args;
                ops::deploy(
                    &ctx,
                    contract_name,
                    label.as_str(),
                    raw.as_ref(),
                    permit_instantiate_only,
                    admin.as_ref(),
                    funds.as_ref().map(|s| s.as_str()).try_into()?,
                    network,
                    timeout_height,
                    {
                        let global_conf = ctx.global_config()?;
                        &Gas::from_args(
                            gas_args,
                            global_conf.gas_price(),
                            global_conf.gas_adjustment(),
                        )?
                    },
                    signer_args.private_key(&ctx.global_config()?)?,
                    signer_args.private_key(&ctx.global_config()?)?,
                    no_rebuild,
                    no_wasm_opt,
                )?;
                Ok(())
            }
            WasmCmd::Upgrade {
                contract_name,
                label,
                raw,
                no_rebuild,
                no_wasm_opt,
                permit_instantiate_only,
                base_tx_args,
            } => {
                let BaseTxArgs {
                    network,
                    signer_args,
                    gas_args,
                    timeout_height,
                }: &BaseTxArgs = base_tx_args;
                ops::upgrade(
                    &ctx,
                    contract_name,
                    label.as_str(),
                    raw.as_ref(),
                    permit_instantiate_only,
                    network,
                    timeout_height,
                    {
                        let global_conf = ctx.global_config()?;
                        &Gas::from_args(
                            gas_args,
                            global_conf.gas_price(),
                            global_conf.gas_adjustment(),
                        )?
                    },
                    signer_args.private_key(&ctx.global_config()?)?,
                    signer_args.private_key(&ctx.global_config()?)?,
                    no_rebuild,
                    no_wasm_opt,
                )?;
                Ok(())
            }
            WasmCmd::Proposal { cmd } => proposal::entrypoint::execute(ctx, cmd),
            WasmCmd::TsGen {
                contract_name,
                schema_gen_cmd,
                schema_dir,
                out_dir,
                node_package_manager,
            } => {
                let root = ctx.root()?;
                let sdk_path = root.join("ts/sdk");
                env::set_current_dir(root.join("contracts").join(contract_name))?;
                if let Some(gen) = schema_gen_cmd {
                    let gen = gen.replace("{contract_name}", contract_name);
                    let split = gen.split(' ').collect::<Vec<&str>>();
                    let mut command = Command::new(split.first().unwrap());
                    run_command(command.args(&split[1..]))?;
                } else {
                    let mut cargo = Command::new("cargo");
                    run_command(cargo.arg("schema"))?;
                };

                if out_dir.is_some() {
                    println!(
                        "    {} {}",
                        style("WARNING:").yellow().bold(),
                        style("`out_dir` is not the default location, skipping typescript bundle")
                            .yellow()
                    );
                    return Ok(());
                }

                env::set_current_dir(sdk_path)?;

                let node_pkg = || Command::new(String::from(node_package_manager));

                let which_node_pkg_manager = run_command(
                    Command::new("which".to_string()).arg::<String>(node_package_manager.into()),
                );

                if !which_node_pkg_manager.is_ok() {
                    bail!("`{}` is required but missing, please install, or if you intended to use another package manager eg. `npm`, please specify different package manager via `--node-package-manager` flag", node_package_manager);
                };

                run_command(node_pkg().arg("run").arg("codegen"))?;
                run_command(node_pkg().arg("install"))?;
                run_command(node_pkg().arg("run").arg("build"))?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::Path};

    use assert_fs::{prelude::*, TempDir};
    use cargo_toml::{Dependency, DependencyDetail, Manifest};
    use predicates::prelude::*;
    use serial_test::serial;

    use super::*;

    struct WasmContext {}
    impl<'a> Context<'a, WasmConfig> for WasmContext {}

    #[test]
    #[serial]
    fn generate_contract_with_default_version_and_path() {
        let temp = setup();

        temp.child("contracts").assert(predicate::path::missing());
        temp.child("contracts/counter-1")
            .assert(predicate::path::missing());
        temp.child("contracts/counter-2")
            .assert(predicate::path::missing());

        WasmModule::execute(
            WasmContext {},
            &WasmCmd::New {
                contract_name: "counter-1".to_string(),
                version: None,
                target_dir: None,
            },
        )
        .unwrap();
        temp.child("contracts/counter-1")
            .assert(predicate::path::exists());

        // cd into contract before running command
        env::set_current_dir(temp.to_path_buf().join(PathBuf::from("contracts"))).unwrap();

        WasmModule::execute(
            WasmContext {},
            &WasmCmd::New {
                contract_name: "counter-2".to_string(),
                target_dir: None,
                version: None,
            },
        )
        .unwrap();
        temp.child("contracts/counter-2")
            .assert(predicate::path::exists());

        temp.close().unwrap();
    }

    #[test]
    #[serial]
    fn generate_contract_with_default_version_and_path_from_child_dir() {
        let temp = setup();

        temp.child("contracts").assert(predicate::path::missing());
        temp.child("contracts/counter-1")
            .assert(predicate::path::missing());
        temp.child("contracts/counter-2")
            .assert(predicate::path::missing());

        WasmModule::execute(
            WasmContext {},
            &WasmCmd::New {
                contract_name: "counter-1".to_string(),
                target_dir: None,
                version: None,
            },
        )
        .unwrap();
        temp.child("contracts/counter-1")
            .assert(predicate::path::exists());

        WasmModule::execute(
            WasmContext {},
            &WasmCmd::New {
                contract_name: "counter-2".to_string(),
                target_dir: None,
                version: None,
            },
        )
        .unwrap();
        temp.child("contracts/counter-2")
            .assert(predicate::path::exists());

        temp.close().unwrap();
    }

    #[test]
    #[serial]
    fn generate_contract_with_custom_version() {
        let temp = setup();

        temp.child("contracts").assert(predicate::path::missing());
        temp.child("contracts/counter-1")
            .assert(predicate::path::missing());
        temp.child("contracts/counter-2")
            .assert(predicate::path::missing());

        WasmModule::execute(
            WasmContext {},
            &WasmCmd::New {
                contract_name: "counter-1".to_string(),
                target_dir: None,
                version: Some("0.16".into()),
            },
        )
        .unwrap();
        temp.child("contracts/counter-1")
            .assert(predicate::path::exists());
        assert_version(Path::new("contracts/counter-1/Cargo.toml"), "0.16");

        WasmModule::execute(
            WasmContext {},
            &WasmCmd::New {
                contract_name: "counter-2".to_string(),
                target_dir: None,
                version: Some("0.16".into()),
            },
        )
        .unwrap();
        temp.child("contracts/counter-2")
            .assert(predicate::path::exists());
        assert_version(Path::new("contracts/counter-2/Cargo.toml"), "0.16");

        temp.close().unwrap();
    }

    #[test]
    #[serial]
    fn generate_contract_with_custom_path() {
        let temp = setup();
        env::set_current_dir(&temp).unwrap();

        temp.child("custom-path").assert(predicate::path::missing());
        temp.child("custom-path/counter-1")
            .assert(predicate::path::missing());
        temp.child("custom-path/counter-2")
            .assert(predicate::path::missing());

        WasmModule::execute(
            WasmContext {},
            &WasmCmd::New {
                contract_name: "counter-1".to_string(),
                target_dir: Some("custom-path".into()),
                version: None,
            },
        )
        .unwrap();
        temp.child("custom-path/counter-1")
            .assert(predicate::path::exists());

        WasmModule::execute(
            WasmContext {},
            &WasmCmd::New {
                contract_name: "counter-2".to_string(),
                target_dir: Some("custom-path".into()),
                version: None,
            },
        )
        .unwrap();
        temp.child("custom-path/counter-2")
            .assert(predicate::path::exists());

        temp.close().unwrap();
    }

    fn setup() -> TempDir {
        let temp = assert_fs::TempDir::new().unwrap();
        env::set_current_dir(&temp).unwrap();
        fs::File::create("Beaker.toml").unwrap();
        temp
    }

    fn assert_version(cargo_toml_path: &Path, expected_version: &str) {
        let manifest = Manifest::from_path(cargo_toml_path).unwrap();
        let version = {
            if let Dependency::Detailed(DependencyDetail {
                version: Some(version),
                ..
            }) = manifest.dependencies.get("cosmwasm-std").unwrap()
            {
                version
            } else {
                ""
            }
        };

        assert!(version.starts_with(expected_version))
    }
}
