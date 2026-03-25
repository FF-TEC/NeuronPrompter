// =============================================================================
// Chain domain entity.
//
// A chain links multiple existing prompts in a defined order. Each chain has
// its own metadata (title, description, tags, etc.) but its content is
// dynamically composed from its sub-prompts joined by a configurable separator.
// Changes to sub-prompts are automatically reflected (live references).
// =============================================================================

use std::fmt;

use serde::{Deserialize, Serialize};

use super::category::Category;
use super::collection::Collection;
use super::prompt::Prompt;
use super::script::Script;
use super::tag::Tag;

/// Discriminant for chain step types. Determines whether a step references a
/// prompt or a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepType {
    Prompt,
    Script,
}

impl StepType {
    /// Returns the string representation used in the database and API.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Script => "script",
        }
    }

    /// Parses a step type from a database string value.
    /// Callers that have a `&str` and want a `Result` can use `.parse::<StepType>()`
    /// via the `FromStr` implementation instead.
    ///
    /// # Errors
    ///
    /// Returns `None` if the value is not `"prompt"` or `"script"`.
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "prompt" => Some(Self::Prompt),
            "script" => Some(Self::Script),
            _ => None,
        }
    }
}

impl std::str::FromStr for StepType {
    type Err = String;

    /// Parses a `StepType` from the string values "prompt" or "script".
    /// Returns an error containing the unrecognized value for any other input.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prompt" => Ok(Self::Prompt),
            "script" => Ok(Self::Script),
            other => Err(format!("unknown step type: '{other}'")),
        }
    }
}

impl fmt::Display for StepType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Persisted chain record containing all columns from the chains table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chain {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub language: Option<String>,
    pub separator: String,
    pub is_favorite: bool,
    pub is_archived: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// A single step in a chain, referencing either a prompt or a script at a
/// specific position. The `step_type` discriminant determines which foreign
/// key is active.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStep {
    pub id: i64,
    pub chain_id: i64,
    pub step_type: StepType,
    pub prompt_id: Option<i64>,
    pub script_id: Option<i64>,
    pub position: i32,
}

/// A chain step with its resolved entity data for display purposes.
/// Exactly one of `prompt` or `script` is present based on `step.step_type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedChainStep {
    pub step: ChainStep,
    pub prompt: Option<Prompt>,
    pub script: Option<Script>,
}

/// Input for creating or replacing a chain step. Used in create and update
/// payloads to specify mixed prompt/script step sequences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStepInput {
    pub step_type: StepType,
    pub item_id: i64,
}

/// A chain together with its resolved steps and taxonomy associations.
/// Used for detail views where the full entity graph is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainWithSteps {
    pub chain: Chain,
    pub steps: Vec<ResolvedChainStep>,
    pub tags: Vec<Tag>,
    pub categories: Vec<Category>,
    pub collections: Vec<Collection>,
}

/// Payload for creating a chain. Steps can be specified either via the legacy
/// `prompt_ids` vector (prompt-only) or the new `steps` vector (mixed types).
/// When `steps` is provided it takes precedence over `prompt_ids`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewChain {
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub language: Option<String>,
    pub separator: Option<String>,
    /// Legacy field: ordered sequence of prompt IDs. Ignored when `steps` is set.
    #[serde(default)]
    pub prompt_ids: Vec<i64>,
    /// Mixed step sequence supporting both prompts and scripts. Takes precedence
    /// over `prompt_ids` when provided.
    #[serde(default)]
    pub steps: Vec<ChainStepInput>,
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    #[serde(default)]
    pub category_ids: Vec<i64>,
    #[serde(default)]
    pub collection_ids: Vec<i64>,
}

/// Payload for updating a chain. All fields are optional; only supplied fields
/// are modified. When `steps` or `prompt_ids` is provided, it replaces the
/// entire step sequence (`steps` takes precedence).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChain {
    pub chain_id: i64,
    pub title: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::deserialize_optional_field"
    )]
    pub description: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::deserialize_optional_field"
    )]
    pub notes: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::deserialize_optional_field"
    )]
    pub language: Option<Option<String>>,
    pub separator: Option<String>,
    /// Legacy field: replaces steps with prompt-only sequence. Ignored when
    /// `steps` is set.
    pub prompt_ids: Option<Vec<i64>>,
    /// Mixed step sequence. Takes precedence over `prompt_ids` when provided.
    pub steps: Option<Vec<ChainStepInput>>,
    pub tag_ids: Option<Vec<i64>>,
    pub category_ids: Option<Vec<i64>>,
    pub collection_ids: Option<Vec<i64>>,
}

/// Filter criteria for listing chains. All fields are optional and combined
/// with AND semantics. Unset fields impose no restriction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChainFilter {
    pub user_id: Option<i64>,
    pub is_favorite: Option<bool>,
    pub is_archived: Option<bool>,
    pub collection_id: Option<i64>,
    pub category_id: Option<i64>,
    pub tag_id: Option<i64>,
    /// Maximum number of results to return (default 200, max 1000).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Number of results to skip for pagination.
    #[serde(default)]
    pub offset: Option<i64>,
}
