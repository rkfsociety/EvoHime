//! Core-owned guided capability recipe catalog.
//!
//! Recipe descriptors are immutable references to existing workflow owners.
//! This module does not execute workflows, call providers, or persist inputs.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Version of the Core-held recipe catalog contract.
pub const CATALOG_VERSION: u32 = 1;
/// Number of built-in guided recipe descriptors.
pub const BUILTIN_RECIPE_COUNT: usize = 8;
/// Maximum number of declared inputs in one descriptor.
pub const MAX_RECIPE_INPUTS: usize = 16;
/// Maximum input length in Unicode scalar values.
pub const MAX_RECIPE_INPUT_CHARS: usize = 512;
/// Maximum preview lines in one descriptor.
pub const MAX_RECIPE_PREVIEW_LINES: usize = 16;
/// Maximum revision-owner entries exposed by one preflight.
pub const MAX_RECIPE_REVISIONS: usize = 32;

/// Guided category used to group recipes in the desktop UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRecipeCategory {
    /// Comparing behavior and evidence for models.
    ModelComparison,
    /// Comparing candidate prompt strategies.
    PromptVariants,
    /// Producing and validating typed outputs.
    StructuredOutput,
    /// Using registered tools under their existing policy.
    ToolUse,
    /// Grounding work in existing knowledge sources.
    KnowledgeGrounding,
    /// Coordinating multiple reviewers.
    MultiAgentReview,
    /// Reviewing local model fit evidence.
    LocalModelFit,
    /// Inspecting security and permission boundaries.
    TrustBoundaries,
}

/// Expected complexity of a guided recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRecipeDifficulty {
    /// A short, read-only guided workflow.
    Introductory,
    /// A workflow with multiple evidence sources or roles.
    Intermediate,
    /// A workflow with explicit policy and approval boundaries.
    Advanced,
}

/// Current ability to start a recipe through an existing workflow owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CapabilityRecipeAvailability {
    /// The referenced workflow template is present and its bindings validate.
    Ready,
    /// No compatible workflow adapter exists in this build.
    Unsupported {
        /// Stable reason code suitable for IPC and UI presentation.
        reason_code: String,
    },
}

/// Result state from the bounded recipe preflight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRecipePreflightState {
    /// All currently observable requirements are available.
    Ready,
    /// Static bindings are valid; a dynamic runtime owner is rechecked at start.
    ReadyWithWarnings,
    /// A required capability or current policy blocks this binding.
    Blocked,
    /// No compatible existing workflow adapter is available.
    Unsupported,
    /// The recipe, input, or pinned definition is invalid.
    InvalidDefinition,
}

/// Safe preflight projection used to confirm one exact recipe/input snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRecipePreflight {
    /// Version of the Core-owned catalog used for this result.
    pub catalog_version: u32,
    /// Stable recipe id, or an empty string when the request is unknown.
    pub recipe_id: String,
    /// Exact recipe version, or zero when the request is unknown.
    pub recipe_version: u32,
    /// Canonical recipe digest.
    pub recipe_hash: String,
    /// Current readiness state.
    pub state: CapabilityRecipePreflightState,
    /// Stable, bounded reason codes.
    pub reason_codes: Vec<String>,
    /// Exact workflow binding when the recipe is runnable.
    pub workflow_binding: Option<CapabilityRecipeWorkflowBinding>,
    /// Digest of the bounded input values; input values themselves are omitted.
    pub input_hash: String,
    /// Digest of the selected workspace; the raw local path is omitted.
    pub workspace_hash: String,
    /// Digest of the instantiated graph snapshot.
    pub run_graph_hash: String,
    /// Safe preview lines supplied by the Core descriptor.
    pub preview: Vec<String>,
    /// Required capabilities from the exact workflow owner.
    pub required_capabilities: Vec<String>,
    /// Optional capabilities from the exact workflow owner.
    pub optional_capabilities: Vec<String>,
    /// Known immutable definition pins plus unpinned dynamic owners.
    pub revisions: Vec<CapabilityRecipeRevision>,
    /// Resource limits from the exact instantiated workflow snapshot.
    pub workflow_budget: Option<crate::workflow::WorkflowBudget>,
    /// Workflow nodes that require explicit human approval.
    pub approval_points: Vec<String>,
    /// Dynamic owners whose state can change or degrade between preflight and execution.
    pub degraded_paths: Vec<String>,
    /// Digest binding the recipe, graph, inputs and current policy together.
    pub preflight_hash: String,
}

/// Whether a required external revision is pinned in the workflow snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRecipeRevisionState {
    /// The exact owner revision is stored in the recipe or workflow snapshot.
    Pinned,
    /// The owner does not persist a revision needed for exact reproduction.
    NotPinned,
}

/// Safe projection of one recipe, template, or dynamic execution owner revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRecipeRevision {
    /// Stable owner kind, such as `recipe`, `workflow_template`, or `context_snapshot`.
    pub owner_kind: String,
    /// Stable owner identifier; never a path, prompt, or credential.
    pub owner_id: String,
    /// Exact revision when the owner exposes one.
    pub revision: Option<String>,
    /// Content digest when the owner exposes one.
    pub content_hash: Option<String>,
    /// Whether exact replay can rely on this revision.
    pub state: CapabilityRecipeRevisionState,
    /// Stable reason code when the revision is unavailable.
    pub reason_code: String,
}

/// One bounded user input declared by a recipe descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRecipeInput {
    /// Stable input field name.
    pub name: String,
    /// Localized label shown by the guided UI.
    pub title: String,
    /// Whether the input must be supplied before preflight.
    pub required: bool,
    /// Maximum accepted length in Unicode scalar values.
    pub max_chars: usize,
}

/// Exact existing workflow binding used by a runnable recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRecipeWorkflowBinding {
    /// Stable Core workflow template id.
    pub template_id: String,
    /// Exact workflow template version.
    pub template_version: u32,
    /// SHA-256 digest of the uninstantiated template graph.
    pub template_graph_hash: String,
}

/// Immutable Core-owned guided reference to an existing capability owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRecipeDescriptor {
    /// Stable built-in recipe identifier.
    pub id: String,
    /// Immutable recipe definition version.
    pub version: u32,
    /// User-facing category.
    pub category: CapabilityRecipeCategory,
    /// Expected complexity.
    pub difficulty: CapabilityRecipeDifficulty,
    /// Localized recipe name.
    pub title: String,
    /// Bounded explanation of the recipe's purpose.
    pub description: String,
    /// Bounded inputs accepted by the existing workflow template.
    pub inputs: Vec<CapabilityRecipeInput>,
    /// Capabilities required by the referenced workflow template.
    pub required_capabilities: Vec<String>,
    /// Capabilities that can improve the result without being required.
    pub optional_capabilities: Vec<String>,
    /// Exact existing workflow binding, absent when no compatible adapter is
    /// available.
    pub workflow_binding: Option<CapabilityRecipeWorkflowBinding>,
    /// Safe, static preview lines. These contain no user input or prompt body.
    pub preview: Vec<String>,
    /// Availability derived from the exact current owner binding.
    pub availability: CapabilityRecipeAvailability,
    /// SHA-256 digest of this descriptor with `content_hash` cleared.
    pub content_hash: String,
}

/// Catalog construction, descriptor validation, or serialization failure.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CapabilityRecipeError {
    /// A built-in definition violates its bounded static contract.
    #[error("invalid capability recipe: {0}")]
    Invalid(&'static str),
    /// A required workflow template is absent or changed incompatibly.
    #[error("invalid recipe workflow binding: {0}")]
    InvalidBinding(&'static str),
    /// A descriptor could not be serialized for hashing.
    #[error("capability recipe serialization failed")]
    Serialization,
}

#[derive(Clone, Copy)]
struct BuiltinDefinition {
    id: &'static str,
    category: CapabilityRecipeCategory,
    difficulty: CapabilityRecipeDifficulty,
    title: &'static str,
    description: &'static str,
    input_name: &'static str,
    input_title: &'static str,
    template_id: Option<&'static str>,
    unsupported_reason: Option<&'static str>,
    optional_capabilities: &'static [&'static str],
}

const BUILTINS: [BuiltinDefinition; BUILTIN_RECIPE_COUNT] = [
    BuiltinDefinition {
        id: "model-comparison",
        category: CapabilityRecipeCategory::ModelComparison,
        difficulty: CapabilityRecipeDifficulty::Intermediate,
        title: "Сравнение моделей",
        description: "Сравнение допустимо только при наличии реального model-run adapter.",
        input_name: "goal",
        input_title: "Цель сравнения",
        template_id: None,
        unsupported_reason: Some("model_run_adapter_unavailable"),
        optional_capabilities: &[],
    },
    BuiltinDefinition {
        id: "prompt-variants",
        category: CapabilityRecipeCategory::PromptVariants,
        difficulty: CapabilityRecipeDifficulty::Advanced,
        title: "Варианты prompt-стратегии",
        description: "Fixture evaluation не считается live provider результатом.",
        input_name: "goal",
        input_title: "Цель сравнения",
        template_id: None,
        unsupported_reason: Some("workflow_adapter_unavailable"),
        optional_capabilities: &[],
    },
    BuiltinDefinition {
        id: "structured-output",
        category: CapabilityRecipeCategory::StructuredOutput,
        difficulty: CapabilityRecipeDifficulty::Intermediate,
        title: "Структурированный результат",
        description: "Проверка typed child reports через существующий workflow.",
        input_name: "goal",
        input_title: "Задача для typed report",
        template_id: Some("plan-implement-review"),
        unsupported_reason: None,
        optional_capabilities: &[],
    },
    BuiltinDefinition {
        id: "tool-use",
        category: CapabilityRecipeCategory::ToolUse,
        difficulty: CapabilityRecipeDifficulty::Advanced,
        title: "Работа с инструментами",
        description: "Запуск требует готового workflow с зарегистрированными tool bindings.",
        input_name: "goal",
        input_title: "Цель работы",
        template_id: None,
        unsupported_reason: Some("workflow_adapter_unavailable"),
        optional_capabilities: &[],
    },
    BuiltinDefinition {
        id: "knowledge-grounding",
        category: CapabilityRecipeCategory::KnowledgeGrounding,
        difficulty: CapabilityRecipeDifficulty::Introductory,
        title: "Ответ с опорой на знания",
        description: "Использует существующий read-only workflow исследования репозитория.",
        input_name: "question",
        input_title: "Вопрос по репозиторию",
        template_id: Some("repository-research"),
        unsupported_reason: None,
        optional_capabilities: &[],
    },
    BuiltinDefinition {
        id: "multi-agent-review",
        category: CapabilityRecipeCategory::MultiAgentReview,
        difficulty: CapabilityRecipeDifficulty::Intermediate,
        title: "Параллельное security review",
        description: "Две независимые security-проверки с детерминированным fan-in.",
        input_name: "scope",
        input_title: "Область проверки",
        template_id: Some("parallel-security-review"),
        unsupported_reason: None,
        optional_capabilities: &[],
    },
    BuiltinDefinition {
        id: "local-model-fit",
        category: CapabilityRecipeCategory::LocalModelFit,
        difficulty: CapabilityRecipeDifficulty::Intermediate,
        title: "Подходящая локальная модель",
        description: "Fit evidence само по себе не разрешает выбрать или запустить модель.",
        input_name: "goal",
        input_title: "Требуемая задача",
        template_id: None,
        unsupported_reason: Some("model_selection_adapter_unavailable"),
        optional_capabilities: &[],
    },
    BuiltinDefinition {
        id: "trust-boundaries",
        category: CapabilityRecipeCategory::TrustBoundaries,
        difficulty: CapabilityRecipeDifficulty::Advanced,
        title: "Границы доверия",
        description: "Ревью секретов и прав; не является egress enforcement.",
        input_name: "scope",
        input_title: "Область проверки",
        template_id: Some("parallel-security-review"),
        unsupported_reason: None,
        optional_capabilities: &[],
    },
];

/// Builds and validates the stable built-in catalog against existing owners.
///
/// Descriptors without an existing compatible workflow binding are returned
/// as `Unsupported`; they are never silently mapped to a different template.
///
/// # Errors
///
/// Returns an error if a static built-in is malformed or a declared workflow
/// binding no longer validates against the current template and registries.
pub fn catalog() -> Result<Vec<CapabilityRecipeDescriptor>, CapabilityRecipeError> {
    let registry = crate::workflow_registry::WorkflowRegistry::bootstrap();
    let mut descriptors = Vec::with_capacity(BUILTIN_RECIPE_COUNT);
    let mut ids = BTreeSet::new();

    for builtin in BUILTINS {
        if !ids.insert(builtin.id) {
            return Err(CapabilityRecipeError::Invalid("duplicate_id"));
        }

        let (workflow_binding, required_capabilities, preview, availability) =
            if let Some(template_id) = builtin.template_id {
                let template = crate::workflow_templates::template(template_id)
                    .ok_or(CapabilityRecipeError::InvalidBinding("unknown_template"))?;
                if template.version != 1 {
                    return Err(CapabilityRecipeError::InvalidBinding(
                        "recipe_template_version_mismatch",
                    ));
                }
                let graph = template.graph();
                graph
                    .validate()
                    .map_err(|_| CapabilityRecipeError::InvalidBinding("invalid_graph"))?;
                let parent = catalog_validation_parent(graph);
                registry
                    .validate_bindings(graph, &parent)
                    .map_err(|_| CapabilityRecipeError::InvalidBinding("invalid_binding"))?;
                let binding = CapabilityRecipeWorkflowBinding {
                    template_id: template.template_id.clone(),
                    template_version: template.version,
                    template_graph_hash: graph.canonical_hash(),
                };
                (
                    Some(binding),
                    template.required_capabilities.clone(),
                    template.preview.clone(),
                    CapabilityRecipeAvailability::Ready,
                )
            } else {
                (
                    None,
                    Vec::new(),
                    vec!["Нет совместимого workflow adapter в этой версии.".into()],
                    CapabilityRecipeAvailability::Unsupported {
                        reason_code: builtin
                            .unsupported_reason
                            .unwrap_or("workflow_adapter_unavailable")
                            .into(),
                    },
                )
            };

        let mut descriptor = CapabilityRecipeDescriptor {
            id: builtin.id.into(),
            version: 1,
            category: builtin.category,
            difficulty: builtin.difficulty,
            title: builtin.title.into(),
            description: builtin.description.into(),
            inputs: vec![CapabilityRecipeInput {
                name: builtin.input_name.into(),
                title: builtin.input_title.into(),
                required: true,
                max_chars: MAX_RECIPE_INPUT_CHARS,
            }],
            required_capabilities,
            optional_capabilities: builtin
                .optional_capabilities
                .iter()
                .map(|capability| (*capability).into())
                .collect(),
            workflow_binding,
            preview,
            availability,
            content_hash: String::new(),
        };
        descriptor.content_hash = canonical_hash(&descriptor)?;
        validate_descriptor(&descriptor)?;
        descriptors.push(descriptor);
    }

    if descriptors.len() != BUILTIN_RECIPE_COUNT {
        return Err(CapabilityRecipeError::Invalid("builtin_count"));
    }
    Ok(descriptors)
}

/// Returns a descriptor by stable id from the current catalog.
pub fn descriptor(id: &str) -> Result<Option<CapabilityRecipeDescriptor>, CapabilityRecipeError> {
    Ok(catalog()?.into_iter().find(|item| item.id == id))
}

/// Computes a descriptor SHA-256 digest while excluding `content_hash`.
pub fn canonical_hash(
    descriptor: &CapabilityRecipeDescriptor,
) -> Result<String, CapabilityRecipeError> {
    let mut canonical = descriptor.clone();
    canonical.content_hash.clear();
    let bytes = serde_json::to_vec(&canonical).map_err(|_| CapabilityRecipeError::Serialization)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

/// Validates bounds, identity, workflow binding shape, and canonical hash.
pub fn validate_descriptor(
    descriptor: &CapabilityRecipeDescriptor,
) -> Result<(), CapabilityRecipeError> {
    if descriptor.id.trim().is_empty()
        || descriptor.id.len() > 128
        || descriptor.version == 0
        || descriptor.title.trim().is_empty()
        || descriptor.title.chars().count() > MAX_RECIPE_INPUT_CHARS
        || descriptor.description.chars().count() > MAX_RECIPE_INPUT_CHARS
        || descriptor.inputs.len() > MAX_RECIPE_INPUTS
        || descriptor.preview.len() > MAX_RECIPE_PREVIEW_LINES
        || descriptor.required_capabilities.len() > 64
        || descriptor.optional_capabilities.len() > 64
    {
        return Err(CapabilityRecipeError::Invalid("bounds"));
    }

    let mut input_names = BTreeSet::new();
    for input in &descriptor.inputs {
        if input.name.trim().is_empty()
            || input.name.len() > 128
            || input.title.trim().is_empty()
            || input.title.chars().count() > MAX_RECIPE_INPUT_CHARS
            || input.max_chars == 0
            || input.max_chars > MAX_RECIPE_INPUT_CHARS
            || !input_names.insert(input.name.as_str())
        {
            return Err(CapabilityRecipeError::Invalid("input"));
        }
    }

    match (&descriptor.workflow_binding, &descriptor.availability) {
        (Some(binding), CapabilityRecipeAvailability::Ready)
            if !binding.template_id.trim().is_empty()
                && binding.template_id.len() <= 128
                && binding.template_version > 0
                && binding.template_graph_hash.len() == 64 => {}
        (None, CapabilityRecipeAvailability::Unsupported { reason_code })
            if !reason_code.trim().is_empty() && reason_code.len() <= 128 => {}
        _ => {
            return Err(CapabilityRecipeError::Invalid(
                "availability_binding_mismatch",
            ))
        }
    }

    if descriptor.content_hash.is_empty() || canonical_hash(descriptor)? != descriptor.content_hash
    {
        return Err(CapabilityRecipeError::Invalid("content_hash"));
    }
    Ok(())
}

fn catalog_validation_parent(
    graph: &crate::workflow::WorkflowGraph,
) -> crate::workflow_registry::ParentCapabilities {
    // This broad parent exists only to validate that a built-in's declared
    // graph is internally resolvable. Runtime admission still uses the fixed,
    // narrower capabilities supplied by the ordinary workflow IPC path.
    let grants = graph
        .nodes
        .iter()
        .filter_map(|node| match &node.node_type {
            crate::workflow::NodeType::Child { child } => Some(child.grants.iter().cloned()),
            _ => None,
        })
        .flatten()
        .collect::<BTreeSet<_>>();
    crate::workflow_registry::ParentCapabilities {
        grants,
        budget: crate::workflow::NodeBudget {
            max_tokens: u64::MAX,
            max_seconds: u64::MAX,
            max_tool_calls: u64::MAX,
        },
        context_allowlist: BTreeSet::new(),
    }
}

/// Validates bounded user values against a descriptor's declared input schema.
///
/// # Errors
///
/// Returns stable errors for unknown, missing, or oversized input values.
pub fn validate_inputs(
    descriptor: &CapabilityRecipeDescriptor,
    inputs: &BTreeMap<String, String>,
) -> Result<(), CapabilityRecipeError> {
    validate_descriptor(descriptor)?;
    if inputs.len() > MAX_RECIPE_INPUTS {
        return Err(CapabilityRecipeError::Invalid("too_many_inputs"));
    }
    for (name, value) in inputs {
        let input = descriptor
            .inputs
            .iter()
            .find(|input| input.name == *name)
            .ok_or(CapabilityRecipeError::Invalid("unknown_input"))?;
        if value.chars().count() > input.max_chars {
            return Err(CapabilityRecipeError::Invalid("input_too_long"));
        }
        if input.required && value.trim().is_empty() {
            return Err(CapabilityRecipeError::Invalid("empty_input"));
        }
    }
    for input in &descriptor.inputs {
        if input.required && !inputs.contains_key(&input.name) {
            return Err(CapabilityRecipeError::Invalid("missing_input"));
        }
    }
    Ok(())
}

/// Builds a redacted preflight for one pinned recipe and bounded input set.
pub fn preflight(
    recipe_id: &str,
    recipe_version: u32,
    expected_recipe_hash: &str,
    inputs: &BTreeMap<String, String>,
    workspace_path: &str,
    registry: &crate::workflow_registry::WorkflowRegistry,
    parent: &crate::workflow_registry::ParentCapabilities,
) -> CapabilityRecipePreflight {
    let input_hash = serde_json::to_vec(inputs)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .unwrap_or_default();
    let mut result = CapabilityRecipePreflight {
        catalog_version: CATALOG_VERSION,
        recipe_id: recipe_id.chars().take(128).collect(),
        recipe_version,
        recipe_hash: String::new(),
        state: CapabilityRecipePreflightState::InvalidDefinition,
        reason_codes: Vec::new(),
        workflow_binding: None,
        input_hash,
        workspace_hash: hex::encode(Sha256::digest(workspace_path.as_bytes())),
        run_graph_hash: String::new(),
        preview: Vec::new(),
        required_capabilities: Vec::new(),
        optional_capabilities: Vec::new(),
        revisions: Vec::new(),
        workflow_budget: None,
        approval_points: Vec::new(),
        degraded_paths: Vec::new(),
        preflight_hash: String::new(),
    };

    if workspace_path.trim().is_empty() || workspace_path.len() > 32 * 1024 {
        result.reason_codes.push("invalid_workspace".into());
        seal_preflight(&mut result, parent);
        return result;
    }

    let recipes = match catalog() {
        Ok(recipes) => recipes,
        Err(_) => {
            result.reason_codes.push("catalog_invalid".into());
            seal_preflight(&mut result, parent);
            return result;
        }
    };
    let Some(recipe) = recipes.into_iter().find(|item| item.id == recipe_id) else {
        result.reason_codes.push("unknown_recipe".into());
        seal_preflight(&mut result, parent);
        return result;
    };
    result.recipe_hash = recipe.content_hash.clone();
    result.recipe_id = recipe.id.clone();
    result.recipe_version = recipe.version;
    result.workflow_binding = recipe.workflow_binding.clone();
    result.preview = recipe.preview.clone();
    result.required_capabilities = recipe.required_capabilities.clone();
    result.optional_capabilities = recipe.optional_capabilities.clone();
    result.revisions.push(pinned_revision(
        "recipe",
        &recipe.id,
        recipe.version.to_string(),
        recipe.content_hash.clone(),
    ));

    if recipe_version != recipe.version || expected_recipe_hash != recipe.content_hash.as_str() {
        result.reason_codes.push("recipe_revision_mismatch".into());
        seal_preflight(&mut result, parent);
        return result;
    }
    if let Err(error) = validate_inputs(&recipe, inputs) {
        result.reason_codes.push(error.code().into());
        seal_preflight(&mut result, parent);
        return result;
    }
    if let CapabilityRecipeAvailability::Unsupported { reason_code } = &recipe.availability {
        result.state = CapabilityRecipePreflightState::Unsupported;
        result.reason_codes.push(reason_code.clone());
        seal_preflight(&mut result, parent);
        return result;
    }
    let Some(binding) = recipe.workflow_binding.as_ref() else {
        result.reason_codes.push("workflow_binding_missing".into());
        seal_preflight(&mut result, parent);
        return result;
    };
    let Some(template) = crate::workflow_templates::template(&binding.template_id) else {
        result
            .reason_codes
            .push("workflow_template_unavailable".into());
        seal_preflight(&mut result, parent);
        return result;
    };
    if template.version != binding.template_version
        || template.graph().canonical_hash() != binding.template_graph_hash
    {
        result
            .reason_codes
            .push("workflow_revision_mismatch".into());
        seal_preflight(&mut result, parent);
        return result;
    }
    result.revisions.push(pinned_revision(
        "workflow_template",
        &template.template_id,
        template.version.to_string(),
        template.graph().canonical_hash(),
    ));
    let graph = match template.instantiate(inputs) {
        Ok(graph) => graph,
        Err(error) => {
            result.reason_codes.push(error.code().into());
            seal_preflight(&mut result, parent);
            return result;
        }
    };
    let expanded = match registry.expand_subgraphs(&graph) {
        Ok(graph) => graph,
        Err(errors) => {
            result.state = CapabilityRecipePreflightState::Blocked;
            result.reason_codes = errors
                .iter()
                .map(|error| error.code().to_string())
                .take(16)
                .collect();
            seal_preflight(&mut result, parent);
            return result;
        }
    };
    if expanded.validate().is_err() {
        result.reason_codes.push("invalid_workflow_graph".into());
        seal_preflight(&mut result, parent);
        return result;
    }
    if let Err(errors) = registry.validate_bindings(&expanded, parent) {
        result.state = CapabilityRecipePreflightState::Blocked;
        result.reason_codes = errors
            .iter()
            .map(|error| error.code().to_string())
            .take(16)
            .collect();
        seal_preflight(&mut result, parent);
        return result;
    }

    result.run_graph_hash = expanded.canonical_hash();
    result.revisions.push(pinned_revision(
        "workflow_graph",
        &expanded.graph_id,
        expanded.version.to_string(),
        result.run_graph_hash.clone(),
    ));
    result.workflow_budget = Some(expanded.budget);
    result.approval_points = expanded
        .nodes
        .iter()
        .filter(|node| node.execution.approval.required)
        .map(|node| node.id.clone())
        .take(MAX_RECIPE_REVISIONS)
        .collect();
    let (revisions, degraded_paths) = project_dynamic_owners(&expanded);
    result.revisions.extend(revisions);
    result.revisions.truncate(MAX_RECIPE_REVISIONS);
    result.degraded_paths = degraded_paths;
    if !result.degraded_paths.is_empty() {
        result.state = CapabilityRecipePreflightState::ReadyWithWarnings;
        result
            .reason_codes
            .extend(result.degraded_paths.iter().cloned());
    } else {
        result.state = CapabilityRecipePreflightState::Ready;
    }
    seal_preflight(&mut result, parent);
    result
}

impl CapabilityRecipeError {
    /// Returns a stable bounded error code for preflight and IPC projections.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(code) => code,
            Self::InvalidBinding(code) => code,
            Self::Serialization => "serialization_failed",
        }
    }
}

fn seal_preflight(
    result: &mut CapabilityRecipePreflight,
    parent: &crate::workflow_registry::ParentCapabilities,
) {
    result.reason_codes.sort();
    result.reason_codes.dedup();
    let mut digest = Sha256::new();
    for value in [
        result.catalog_version.to_string(),
        result.recipe_id.clone(),
        result.recipe_version.to_string(),
        result.recipe_hash.clone(),
        format!("{:?}", result.state),
        result.input_hash.clone(),
        result.workspace_hash.clone(),
        result.run_graph_hash.clone(),
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    for reason in &result.reason_codes {
        digest.update(reason.as_bytes());
        digest.update([0]);
    }
    for grant in &parent.grants {
        digest.update(grant.as_bytes());
        digest.update([0]);
    }
    digest.update(parent.budget.max_tokens.to_le_bytes());
    digest.update(parent.budget.max_seconds.to_le_bytes());
    digest.update(parent.budget.max_tool_calls.to_le_bytes());
    for item in &parent.context_allowlist {
        digest.update(item.as_bytes());
        digest.update([0]);
    }
    for revision in &result.revisions {
        for value in [
            revision.owner_kind.as_str(),
            revision.owner_id.as_str(),
            revision.revision.as_deref().unwrap_or_default(),
            revision.content_hash.as_deref().unwrap_or_default(),
            match revision.state {
                CapabilityRecipeRevisionState::Pinned => "pinned",
                CapabilityRecipeRevisionState::NotPinned => "not_pinned",
            },
            revision.reason_code.as_str(),
        ] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }
    }
    if let Some(budget) = result.workflow_budget {
        digest.update(budget.max_parallel_nodes.to_le_bytes());
        digest.update(budget.max_tokens.to_le_bytes());
        digest.update(budget.max_tool_calls.to_le_bytes());
        digest.update(budget.max_wall_clock_ms.to_le_bytes());
    }
    for point in &result.approval_points {
        digest.update(point.as_bytes());
        digest.update([0]);
    }
    for path in &result.degraded_paths {
        digest.update(path.as_bytes());
        digest.update([0]);
    }
    result.preflight_hash = format!("sha256:{}", hex::encode(digest.finalize()));
}

fn pinned_revision(
    owner_kind: &str,
    owner_id: &str,
    revision: String,
    content_hash: String,
) -> CapabilityRecipeRevision {
    CapabilityRecipeRevision {
        owner_kind: owner_kind.into(),
        owner_id: owner_id.into(),
        revision: Some(revision),
        content_hash: Some(content_hash),
        state: CapabilityRecipeRevisionState::Pinned,
        reason_code: String::new(),
    }
}

fn unpinned_revision(
    owner_kind: &str,
    owner_id: &str,
    reason_code: &str,
) -> CapabilityRecipeRevision {
    CapabilityRecipeRevision {
        owner_kind: owner_kind.into(),
        owner_id: owner_id.into(),
        revision: None,
        content_hash: None,
        state: CapabilityRecipeRevisionState::NotPinned,
        reason_code: reason_code.into(),
    }
}

fn project_dynamic_owners(
    graph: &crate::workflow::WorkflowGraph,
) -> (Vec<CapabilityRecipeRevision>, Vec<String>) {
    use crate::workflow::NodeType;

    let mut revisions = Vec::new();
    let mut degraded_paths = BTreeSet::new();
    let mut owners = BTreeSet::new();
    for node in &graph.nodes {
        let (owner_kind, owner_id, reason_code, degraded_path) = match &node.node_type {
            NodeType::Child { child } => (
                "model_executor",
                child.role.as_str(),
                "model_revision_not_pinned",
                "child_executor_rechecked_at_start",
            ),
            NodeType::ContextProvider { provider } => (
                "context_snapshot",
                provider.provider_id.as_str(),
                "context_revision_not_pinned",
                "context_snapshot_rechecked_at_execution",
            ),
            NodeType::Research => (
                "research_snapshot",
                "workspace.research",
                "research_revision_not_pinned",
                "research_evidence_rechecked_at_execution",
            ),
            NodeType::Tool { tool } => (
                "tool_executor",
                tool.tool_name.as_str(),
                "tool_revision_not_pinned",
                "tool_policy_rechecked_at_execution",
            ),
            NodeType::McpTool { mcp } => (
                "mcp_executor",
                mcp.server_id.as_str(),
                "mcp_revision_not_pinned",
                "mcp_policy_rechecked_at_execution",
            ),
            NodeType::IntegrationAction { integration } => (
                "integration_executor",
                integration.provider_id.as_str(),
                "integration_revision_not_pinned",
                "integration_policy_rechecked_at_execution",
            ),
            _ => continue,
        };
        degraded_paths.insert(degraded_path.to_string());
        if owners.insert((owner_kind, owner_id)) && revisions.len() < MAX_RECIPE_REVISIONS {
            revisions.push(unpinned_revision(owner_kind, owner_id, reason_code));
        }
    }
    (revisions, degraded_paths.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_eight_stable_descriptors_and_valid_hashes() {
        let recipes = catalog().expect("built-in catalog validates");
        let ids = recipes
            .iter()
            .map(|recipe| recipe.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(recipes.len(), BUILTIN_RECIPE_COUNT);
        assert_eq!(
            ids,
            vec![
                "model-comparison",
                "prompt-variants",
                "structured-output",
                "tool-use",
                "knowledge-grounding",
                "multi-agent-review",
                "local-model-fit",
                "trust-boundaries",
            ]
        );
        for recipe in recipes {
            assert_eq!(canonical_hash(&recipe).expect("hash"), recipe.content_hash);
        }
    }

    #[test]
    fn existing_workflow_bindings_are_exact_and_missing_adapters_are_typed() {
        let recipes = catalog().expect("built-in catalog validates");
        let by_id = recipes
            .into_iter()
            .map(|recipe| (recipe.id.clone(), recipe))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            by_id["knowledge-grounding"]
                .workflow_binding
                .as_ref()
                .expect("bound")
                .template_id,
            "repository-research"
        );
        assert_eq!(
            by_id["multi-agent-review"]
                .workflow_binding
                .as_ref()
                .expect("bound")
                .template_id,
            "parallel-security-review"
        );
        assert!(matches!(
            &by_id["model-comparison"].availability,
            CapabilityRecipeAvailability::Unsupported { .. }
        ));
        assert!(matches!(
            &by_id["local-model-fit"].availability,
            CapabilityRecipeAvailability::Unsupported { .. }
        ));
    }

    #[test]
    fn inputs_are_bounded_and_match_the_declared_schema() {
        let recipe = descriptor("knowledge-grounding")
            .expect("catalog")
            .expect("recipe");
        assert!(validate_inputs(
            &recipe,
            &BTreeMap::from([("question".into(), "How does recovery work?".into())])
        )
        .is_ok());
        assert_eq!(
            validate_inputs(&recipe, &BTreeMap::from([("unknown".into(), "x".into())])),
            Err(CapabilityRecipeError::Invalid("unknown_input"))
        );
    }

    #[test]
    fn preflight_is_redacted_and_rejects_stale_or_unsupported_recipes() {
        let registry = crate::workflow_registry::WorkflowRegistry::bootstrap();
        let parent = crate::ipc_bridge::workflow_parent_capabilities();
        let recipe = descriptor("knowledge-grounding")
            .expect("catalog")
            .expect("recipe");
        let inputs = BTreeMap::from([("question".into(), "private user question".into())]);
        let ready = preflight(
            &recipe.id,
            recipe.version,
            &recipe.content_hash,
            &inputs,
            r"C:\repo",
            &registry,
            &parent,
        );
        assert_eq!(
            ready.state,
            CapabilityRecipePreflightState::ReadyWithWarnings
        );
        assert!(ready.workflow_budget.is_some());
        assert!(ready
            .revisions
            .iter()
            .any(|revision| revision.owner_kind == "context_snapshot"
                && revision.state == CapabilityRecipeRevisionState::NotPinned));
        assert!(ready
            .degraded_paths
            .contains(&"context_snapshot_rechecked_at_execution".into()));
        assert!(!serde_json::to_string(&ready)
            .expect("projection serializes")
            .contains("private user question"));
        assert_eq!(ready.run_graph_hash.len(), 64);
        assert_eq!(ready.input_hash.len(), 64);

        let stale = preflight(
            &recipe.id,
            recipe.version + 1,
            &recipe.content_hash,
            &inputs,
            r"C:\repo",
            &registry,
            &parent,
        );
        assert_eq!(
            stale.state,
            CapabilityRecipePreflightState::InvalidDefinition
        );
        assert!(stale
            .reason_codes
            .contains(&"recipe_revision_mismatch".into()));

        let unsupported_recipe = descriptor("model-comparison")
            .expect("catalog")
            .expect("recipe");
        let unsupported = preflight(
            &unsupported_recipe.id,
            unsupported_recipe.version,
            &unsupported_recipe.content_hash,
            &BTreeMap::from([("goal".into(), "compare".into())]),
            r"C:\repo",
            &registry,
            &parent,
        );
        assert_eq!(
            unsupported.state,
            CapabilityRecipePreflightState::Unsupported
        );
        assert!(unsupported
            .reason_codes
            .contains(&"model_run_adapter_unavailable".into()));

        let structured = descriptor("structured-output")
            .expect("catalog")
            .expect("recipe");
        let approval = preflight(
            &structured.id,
            structured.version,
            &structured.content_hash,
            &BTreeMap::from([("goal".into(), "typed review".into())]),
            r"C:\repo",
            &registry,
            &parent,
        );
        assert!(approval.approval_points.contains(&"approval".into()));
        assert!(approval.workflow_budget.is_some());
    }
}
