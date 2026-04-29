#![forbid(unsafe_code)]

use patchwright_core::action::Action;
use patchwright_core::error::{PatchwrightError, Result};
use patchwright_core::traits::ModelProvider;
use patchwright_core::types::{
    ArchitectureDesign, ArchitectureFinding, DesignOption, EvidenceRef, FileImpact, ModelRequest,
    ModelResponse, PlanStep, RecommendedDesign, Risk, TestStrategy,
};
use serde_json::Value;
use std::time::Duration;

pub use patchwright_model_contract::{build_openai_prompt as build_prompt, parse_action_json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    DryRun,
    Http,
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    config: OpenAiConfig,
    mode: Mode,
}

impl OpenAiCompatibleClient {
    pub fn new(config: OpenAiConfig) -> Self {
        Self {
            config,
            mode: Mode::Http,
        }
    }

    pub fn dry_run(config: OpenAiConfig) -> Self {
        Self {
            config,
            mode: Mode::DryRun,
        }
    }

    pub fn config(&self) -> &OpenAiConfig {
        &self.config
    }
}

impl ModelProvider for OpenAiCompatibleClient {
    fn propose_action(&mut self, request: ModelRequest) -> Result<ModelResponse> {
        let action = match self.mode {
            Mode::DryRun => Action::Finish {
                summary: format!(
                    "Dry run for model '{}': {}",
                    self.config.model, request.task.text
                ),
            },
            Mode::Http => self.propose_http_action(request)?,
        };

        Ok(ModelResponse { action })
    }

    fn propose_design(&mut self, request: ModelRequest) -> Result<ArchitectureDesign> {
        match self.mode {
            Mode::DryRun => Ok(dry_run_design(&request)),
            Mode::Http => Err(PatchwrightError::Model(
                "OpenAI-compatible architecture design is not implemented yet".to_owned(),
            )),
        }
    }
}

impl OpenAiCompatibleClient {
    fn propose_http_action(&self, request: ModelRequest) -> Result<Action> {
        let api_key = std::env::var(&self.config.api_key_env).map_err(|_| {
            PatchwrightError::Model(format!(
                "OpenAI-compatible API key environment variable '{}' is not set",
                self.config.api_key_env
            ))
        })?;
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let authorization = format!("Bearer {api_key}");
        let body = patchwright_model_contract::build_openai_prompt(&request, &self.config.model);

        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .build();
        let response: Value = agent
            .post(&url)
            .set("Authorization", &authorization)
            .send_json(body)
            .map_err(|error| {
                PatchwrightError::Model(format!("OpenAI-compatible request failed: {error}"))
            })?
            .into_json()
            .map_err(|error| {
                PatchwrightError::Model(format!(
                    "OpenAI-compatible response was not valid JSON: {error}"
                ))
            })?;

        let content = response
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PatchwrightError::Model(
                    "OpenAI-compatible response missing choices[0].message.content".to_string(),
                )
            })?;

        patchwright_model_contract::parse_action_json(content)
    }
}

fn dry_run_design(request: &ModelRequest) -> ArchitectureDesign {
    let title = format!("Feature Design: {}", title_case(&request.task.text));
    let evidence = context_evidence(request);
    let target_files = evidence
        .iter()
        .map(|evidence| evidence.path.clone())
        .collect::<Vec<_>>();

    ArchitectureDesign {
        title,
        goal: request.task.text.clone(),
        current_architecture: vec![ArchitectureFinding {
            summary: "Dry-run design generated from the current context pack.".to_owned(),
            evidence: evidence.clone(),
        }],
        assumptions: vec!["Dry-run mode does not call a model.".to_owned()],
        non_goals: vec!["No code changes are made by design mode.".to_owned()],
        options: vec![DesignOption {
            name: "Incremental implementation".to_owned(),
            summary: "Make the smallest evidence-backed design change first.".to_owned(),
            pros: vec!["Keeps verification scoped.".to_owned()],
            cons: vec!["Requires follow-up implementation steps.".to_owned()],
            evidence: evidence.clone(),
        }],
        recommendation: RecommendedDesign {
            option_name: "Incremental implementation".to_owned(),
            rationale: "Patchwright should design first, then implement verified steps.".to_owned(),
            evidence: evidence.clone(),
        },
        file_impact: target_files
            .iter()
            .cloned()
            .map(|path| FileImpact {
                path,
                change_summary: "Likely affected by the requested task.".to_owned(),
                risk: None,
                evidence: evidence.clone(),
            })
            .collect(),
        implementation_plan: vec![PlanStep {
            id: "step-1".to_owned(),
            title: "Inspect and scope the first implementation slice".to_owned(),
            description: "Use the design evidence to implement one verified change.".to_owned(),
            depends_on: Vec::new(),
            target_files,
            acceptance_criteria: vec!["The selected step has explicit verification.".to_owned()],
            verification_commands: vec!["patchwright verify --repo .".to_owned()],
        }],
        test_strategy: TestStrategy {
            unit: vec!["Add or update focused unit tests for the first slice.".to_owned()],
            integration: Vec::new(),
            end_to_end: Vec::new(),
            manual: Vec::new(),
            commands: vec!["patchwright verify --repo .".to_owned()],
        },
        migration_plan: None,
        rollback_plan: Some(
            "Revert the accepted implementation patch if verification fails.".to_owned(),
        ),
        risks: vec![Risk {
            title: "Insufficient repository evidence".to_owned(),
            impact: "The first plan may miss hidden architecture boundaries.".to_owned(),
            mitigation: "Collect more file evidence before implementation.".to_owned(),
            evidence,
        }],
        open_questions: Vec::new(),
        acceptance_criteria: vec![
            "A human can review this design before implementation.".to_owned()
        ],
    }
}

fn context_evidence(request: &ModelRequest) -> Vec<EvidenceRef> {
    request
        .context
        .as_ref()
        .map(|context| {
            context
                .files
                .iter()
                .take(5)
                .map(|file| EvidenceRef {
                    path: file.path.clone(),
                    start_line: None,
                    end_line: None,
                    reason: "ranked by context retrieval".to_owned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
