use crate::args::{DesignOptions, SolveOptions};
use patchwright_config::ModelProviderKind;
use patchwright_core::error::Result as PatchwrightResult;
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{ArchitectureDesign, ModelRequest, ModelResponse};
use patchwright_model_codex_cli::CodexCliClient;
use patchwright_model_openai::OpenAiCompatibleClient;

pub(crate) enum CliModel {
    OpenAi(OpenAiCompatibleClient),
    CodexCli(CodexCliClient),
}

impl ModelProvider for CliModel {
    fn propose_action(&mut self, request: ModelRequest) -> PatchwrightResult<ModelResponse> {
        match self {
            Self::OpenAi(model) => model.propose_action(request),
            Self::CodexCli(model) => model.propose_action(request),
        }
    }

    fn propose_design(&mut self, request: ModelRequest) -> PatchwrightResult<ArchitectureDesign> {
        match self {
            Self::OpenAi(model) => model.propose_design(request),
            Self::CodexCli(model) => model.propose_design(request),
        }
    }
}

pub(crate) fn cli_model(options: &SolveOptions) -> Result<CliModel, String> {
    if options.dry_run {
        let mut config = options.openai.clone();
        if config.model.is_empty() {
            config.model = "dry-run".to_owned();
        }
        return Ok(CliModel::OpenAi(OpenAiCompatibleClient::dry_run(config)));
    }

    match options.model_provider {
        ModelProviderKind::CodexCli => Ok(CliModel::CodexCli(CodexCliClient::new(
            options.codex_cli.clone(),
        ))),
        ModelProviderKind::OpenAiCompatible => {
            if options.openai.model.is_empty() {
                return Err(
                    "OpenAI-compatible solve requires --model <name> or model.openai.model"
                        .to_owned(),
                );
            }
            Ok(CliModel::OpenAi(OpenAiCompatibleClient::new(
                options.openai.clone(),
            )))
        }
    }
}

pub(crate) fn design_model(options: &DesignOptions) -> Result<CliModel, String> {
    if options.dry_run {
        let mut config = options.openai.clone();
        if config.model.is_empty() {
            config.model = "dry-run".to_owned();
        }
        return Ok(CliModel::OpenAi(OpenAiCompatibleClient::dry_run(config)));
    }

    match options.model_provider {
        ModelProviderKind::CodexCli => Ok(CliModel::CodexCli(CodexCliClient::new(
            options.codex_cli.clone(),
        ))),
        ModelProviderKind::OpenAiCompatible => {
            if options.openai.model.is_empty() {
                return Err(
                    "OpenAI-compatible design requires --model <name> or model.openai.model"
                        .to_owned(),
                );
            }
            Ok(CliModel::OpenAi(OpenAiCompatibleClient::new(
                options.openai.clone(),
            )))
        }
    }
}
