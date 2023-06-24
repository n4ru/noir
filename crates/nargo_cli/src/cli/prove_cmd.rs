use nameof::name_of;

use std::{collections::HashMap};

use clap::Args;
use tracing::debug;

use super::{NargoConfig, backend_vendor_cmd::{BackendCommand, ProofArtifact, WitnessArtifact}};
use crate::{
    errors::CliError, cli::backend_vendor_cmd::{self, execute_backend_cmd}, constants,
};

/// Create proof for this program. The proof is returned as a hex encoded string.
#[derive(Debug, Clone, Args)]
pub(crate) struct ProveCommand {
    /// The name of the proof
    // proof_name: Option<String>,

    /// The name of the circuit build files (ACIR, proving and verification keys)
    // circuit_name: Option<String>,

    /// Verify proof after proving
    // #[arg(short, long)]
    // verify: bool,

    #[clap(flatten)]
    pub(crate) proof_options: ProofArtifact,

    #[clap(flatten)]
    pub(crate) witness_options: WitnessArtifact,

    #[clap(flatten)]
    backend_options: BackendCommand


}

pub(crate) fn run(
    mut args: ProveCommand,
    config: NargoConfig,
) -> Result<i32, CliError> {    

    backend_vendor_cmd::configure_proof_artifact(&config, &mut args.proof_options);

    backend_vendor_cmd::configure_witness_artifact(&config, &mut args.witness_options);

    debug!("Supplied Prove arguments: {:?}", args);

    let backend_executable_path = backend_vendor_cmd::resolve_backend(&args.backend_options, &config)?;
    let mut raw_pass_through= args.backend_options.backend_arguments.unwrap_or_default();
    let mut backend_args = vec![String::from(constants::PROVE_SUB_CMD)];
    backend_args.append(&mut raw_pass_through);

    let mut envs = HashMap::new();
    envs.insert(name_of!(nargo_artifact_path in NargoConfig).to_uppercase(), String::from(config.nargo_artifact_path.unwrap().as_os_str().to_str().unwrap()));
    envs.insert(name_of!(nargo_proof_path in ProofArtifact).to_uppercase(), String::from(args.proof_options.nargo_proof_path.unwrap().as_os_str().to_str().unwrap()));
    envs.insert(name_of!(nargo_verification_key_path in ProofArtifact).to_uppercase(), String::from(args.proof_options.nargo_verification_key_path.unwrap().as_os_str().to_str().unwrap()));
    envs.insert(name_of!(nargo_witness_path in WitnessArtifact).to_uppercase(), String::from(args.witness_options.nargo_witness_path.unwrap().as_os_str().to_str().unwrap()));
    let exit_code = execute_backend_cmd(&backend_executable_path, backend_args, &config.nargo_package_root, Some(envs));

    match exit_code {
        Ok(code) => {
            if code > 0 {
                Err(CliError::Generic(format!("Backend exited with failure code: {}", code)))
            } else {
                Ok(code)
            }
        },
        Err(err) => Err(err),
    }
    
}



