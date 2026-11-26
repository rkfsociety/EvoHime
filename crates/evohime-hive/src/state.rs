//! Read-only aggregated snapshot of the hive for the operations panel.

pub use evohime_hive_contract::state::*;
pub use evohime_model_gateway::ProviderLimits;

use crate::observation::{HiveChildState, MinimalChildWorkflowPayload};
use crate::settings::HiveSettings;
use evohime_hive_contract::HiveModelUsage;

/// Extension methods for constructing [`HiveStateSnapshot`] from Hive-internal projections.
pub trait HiveStateSnapshotExt {
    /// Builds the safe empty snapshot for the given settings.
    fn empty(settings: &HiveSettings) -> Self;
    /// Aggregates the latest child projections into one snapshot.
    fn from_projection(
        settings: &HiveSettings,
        projections: &[MinimalChildWorkflowPayload],
        registry_roles: &[String],
    ) -> Self;
    /// Aggregates child projections and the committed provider observations.
    fn from_observation(
        settings: &HiveSettings,
        projections: &[MinimalChildWorkflowPayload],
        registry_roles: &[String],
        observation: &HiveObservation,
    ) -> Self;
}

impl HiveStateSnapshotExt for HiveStateSnapshot {
    /// Builds the safe empty snapshot for the given settings.
    ///
    /// Limits are still reported: the panel shows the configured ceilings even
    /// before the first task starts.
    fn empty(settings: &HiveSettings) -> Self {
        Self::from_observation(settings, &[], &[], &HiveObservation::default())
    }

    /// Aggregates the latest child projections into one snapshot.
    ///
    /// `registry_roles` lists roles known to the persistent agent registry. They
    /// appear with `active_count = 0` so an idle department stays visible.
    /// Terminal projections are ignored, which keeps the snapshot an
    /// "active hive" view and lets the caller pass every replay window.
    fn from_projection(
        settings: &HiveSettings,
        projections: &[MinimalChildWorkflowPayload],
        registry_roles: &[String],
    ) -> Self {
        Self::from_observation(
            settings,
            projections,
            registry_roles,
            &HiveObservation::default(),
        )
    }

    /// Aggregates child projections and the committed provider observations.
    ///
    /// Running work makes her `active`, while a reached configured rolling cap
    /// reports its reason and exact reset moment when dispatch history allows
    /// Core to compute one.
    fn from_observation(
        settings: &HiveSettings,
        projections: &[MinimalChildWorkflowPayload],
        registry_roles: &[String],
        observation: &HiveObservation,
    ) -> Self {
        let capacity = bounded_u16(settings.effective_worker_ceiling() as usize).max(1);
        let mut departments: std::collections::BTreeMap<&str, u16> =
            std::collections::BTreeMap::new();
        for role in registry_roles {
            departments.entry(role.as_str()).or_insert(0);
        }
        let mut active_agents = Vec::new();
        let mut processing = 0_u16;
        let mut analyzing = 0_u16;
        let mut generating_plan = 0_u16;
        let mut spawned = 0_u64;
        let mut ceiling = 0_u64;
        for projection in projections {
            if !is_active(projection) {
                continue;
            }
            let role = projection.role.as_str();
            *departments.entry(role).or_insert(0) += 1;
            processing = processing.saturating_add(1);
            if role_is_analysis(role) {
                analyzing = analyzing.saturating_add(1);
            }
            if role_is_planning(role) {
                generating_plan = generating_plan.saturating_add(1);
            }
            spawned = spawned.saturating_add(u64::from(projection.spawned_subagents));
            ceiling = ceiling.saturating_add(u64::from(projection.planned_ceiling));
            if active_agents.len() < MAX_HIVE_ACTIVE_AGENTS {
                let display_name = projection
                    .display_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(bounded_id)
                    .unwrap_or_else(|| {
                        fallback_russian_agent_name(role, &projection.child_task_id)
                    });
                active_agents.push(HiveActiveAgent {
                    id: bounded_id(&projection.child_task_id),
                    display_name: Some(display_name),
                    role: bounded_id(role),
                    current_action: current_action(projection.state),
                    depth: projection.depth,
                    spawned_subagents: bounded_u16(projection.spawned_subagents as usize),
                    ceiling: bounded_u16(projection.planned_ceiling as usize),
                });
            }
        }
        active_agents.sort_by(|left, right| left.id.cmp(&right.id));
        let mut departments: Vec<HiveDepartment> = departments
            .into_iter()
            .take(MAX_HIVE_DEPARTMENTS)
            .map(|(role, active_count)| HiveDepartment {
                role: bounded_id(role),
                active_count,
                max_capacity: capacity,
            })
            .collect();
        departments.sort_by(|left, right| left.role.cmp(&right.role));
        let total = active_agents.len();
        let budget = budget_from(observation);
        Self {
            schema_version: HIVE_STATE_SCHEMA_VERSION,
            queen: queen_state(total, &budget, observation),
            metrics: HiveMetrics {
                total_agents_online: bounded_u16(total),
                active_tasks: bounded_u16(processing as usize),
                tasks_today: observation.tasks_today,
                average_task_time_sec: observation.average_task_time_sec,
                efficiency_percent: efficiency_percent(spawned, ceiling),
            },
            departments,
            active_agents,
            task_flow: HiveTaskFlow {
                processing: bounded_u8(processing as usize),
                analyzing: bounded_u8(analyzing as usize),
                generating_plan: bounded_u8(generating_plan as usize),
            },
            limits: limits_for(settings),
            budget,
            daily_budget_usd: settings.daily_budget_usd,
            eva_workspace_path: settings.eva_workspace_path().map(str::to_owned),
            watched_repositories: settings
                .all_workspace_paths()
                .into_iter()
                .map(|path| WatchedRepoProject {
                    path: path.to_owned(),
                    // EvaPulse currently reports lifecycle state globally; a
                    // repository-specific scan marker will be filled here as
                    // soon as the scanner publishes its target path.
                    status: "idle".to_owned(),
                    last_scanned_at_ms: None,
                    current_task: None,
                })
                .collect(),
            hive_provider_profile: settings.hive_provider_profile.clone(),
            hive_model: settings.hive_model.clone(),
            hive_fallback_provider_profile: settings.hive_fallback_provider_profile.clone(),
            hive_fallback_model: settings.hive_fallback_model.clone(),
            hive_provider_limits: settings.hive_provider_limits,
            hive_fallback_provider_limits: settings.hive_fallback_provider_limits,
            hive_daily_token_quota: settings.hive_daily_token_quota,
            hive_fallback_daily_token_quota: settings.hive_fallback_daily_token_quota,
            hive_token_quota_configured: settings.hive_token_quota_configured,
            hive_fallback_token_quota_configured: settings.hive_fallback_token_quota_configured,
            hive_quota_reset_utc_midnight: settings.hive_quota_reset_utc_midnight,
            hive_fallback_quota_reset_utc_midnight: settings.hive_fallback_quota_reset_utc_midnight,
            hive_provider_configs: settings.effective_provider_configs(false),
            hive_fallback_provider_configs: settings.effective_provider_configs(true),
            proactive_evolution_enabled: settings.proactive_evolution_enabled,
            hive_reasoning_effort: settings.hive_reasoning_effort.clone(),
            hive_max_context_tokens: settings.hive_max_context_tokens,
            hive_context_compression_tokens: settings.hive_context_compression_tokens,
            local_observation_interval_secs: settings.local_observation_interval_secs,
            llm_dispatch_interval_secs: settings.llm_dispatch_interval_secs,
            observation_status: observation_status_from(observation),
            next_wake_at_ms: observation.next_observation_at_ms,
        }
    }
}

/// Projects the committed provider observations into the request budget.
fn budget_from(observation: &HiveObservation) -> HiveBudget {
    let minute_cap = observation.minute_cap;
    let hour_cap = observation.hour_cap;
    let four_hour_cap = observation.four_hour_cap;
    let day_cap = observation.day_cap;
    HiveBudget {
        provider: bounded_id(&observation.provider),
        history_available: observation.history_available,
        model: bounded_id(&observation.model),
        requests_last_minute: bounded_u32(observation.requests_last_minute),
        requests_last_hour: bounded_u32(observation.requests_last_hour),
        requests_last_4_hours: bounded_u32(observation.requests_last_4_hours),
        requests_last_day: bounded_u32(observation.requests_last_day),
        minute_cap,
        hour_cap,
        four_hour_cap,
        day_cap,
        limits: ProviderLimits {
            rpm: minute_cap,
            rph: hour_cap,
            rp4h: four_hour_cap,
            rpd: day_cap,
        },
        // A cap of zero is the configured "unlimited" value, never "reached".
        minute_exhausted: minute_cap > 0
            && observation.requests_last_minute >= u64::from(minute_cap),
        hour_exhausted: hour_cap > 0 && observation.requests_last_hour >= u64::from(hour_cap),
        four_hour_exhausted: four_hour_cap > 0
            && observation.requests_last_4_hours >= u64::from(four_hour_cap),
        day_exhausted: day_cap > 0 && observation.requests_last_day >= u64::from(day_cap),
        daily_cost_available: observation.daily_cost_available,
        daily_cost_nano_usd: observation.daily_cost_nano_usd,
        daily_token_quota: observation.daily_token_quota,
        daily_tokens_remaining: observation
            .daily_token_quota
            .map(|quota| quota.saturating_sub(observation.daily_tokens_used)),
        quota_reset_utc_midnight: observation.quota_reset_utc_midnight,
        fallback_provider: observation.fallback_provider.clone(),
        fallback_model: observation
            .fallback_model
            .as_deref()
            .map(bounded_id)
            .filter(|m| !m.is_empty()),
        fallback_tokens_remaining: observation.fallback_tokens_remaining,
        fallback_tokens_used: observation.fallback_tokens_used,
        fallback_daily_token_quota: observation.fallback_daily_token_quota,
        fallback_quota_reset_utc_midnight: observation.fallback_quota_reset_utc_midnight,
    }
}

/// Keeps the observation status inside the published enum.
///
/// An unknown producer value degrades to `disabled` instead of leaking an
/// unbounded string into the UI.
fn observation_status_from(observation: &HiveObservation) -> String {
    match observation.observation_status.as_str() {
        HIVE_OBSERVATION_OBSERVING => HIVE_OBSERVATION_OBSERVING,
        HIVE_OBSERVATION_WAITING_FOR_LIMITS => HIVE_OBSERVATION_WAITING_FOR_LIMITS,
        HIVE_OBSERVATION_WAITING_FOR_TASKS => HIVE_OBSERVATION_WAITING_FOR_TASKS,
        HIVE_OBSERVATION_SLEEPING => HIVE_OBSERVATION_SLEEPING,
        _ => HIVE_OBSERVATION_DISABLED,
    }
    .to_owned()
}

/// DeepSeek tariff for one model, in nano-USD per token.
struct DeepSeekTariff {
    cache_hit_peak: u64,
    cache_hit_off_peak: u64,
    cache_miss_peak: u64,
    cache_miss_off_peak: u64,
    output_peak: u64,
    output_off_peak: u64,
}

/// Resolves the published tariff of a DeepSeek model.
///
/// Prices come from <https://api-docs.deepseek.com/quick_start/pricing/> and are
/// converted from USD per million tokens to nano-USD per token. The retired
/// `deepseek-v4-flash` spellings are still accepted by the provider and billed
/// at the Flash rate. Any other identifier returns `None` instead of applying a
/// possibly incorrect rate.
fn deepseek_tariff(model: &str) -> Option<DeepSeekTariff> {
    match model.trim().to_ascii_lowercase().as_str() {
        "deepseek-flash" | "deepseek-v4-flash" | "deepseek-v4-flash-vision-exp" => {
            Some(DeepSeekTariff {
                cache_hit_peak: 6,
                cache_hit_off_peak: 3,
                cache_miss_peak: 300,
                cache_miss_off_peak: 150,
                output_peak: 1_200,
                output_off_peak: 600,
            })
        }
        "deepseek-v4-pro" => Some(DeepSeekTariff {
            cache_hit_peak: 44,
            cache_hit_off_peak: 22,
            cache_miss_peak: 1_320,
            cache_miss_off_peak: 660,
            output_peak: 3_960,
            output_off_peak: 1_980,
        }),
        _ => None,
    }
}

/// Calculates the daily model charge in nano-USD from user-configured USD-per-1M-token rates.
pub fn custom_daily_cost_nano_usd(
    pricing: &evohime_model_gateway::config::ModelPricingConfig,
    usage: &[HiveModelUsage],
) -> Option<u64> {
    if ![
        pricing.input_usd_per_1m,
        pricing.cached_input_usd_per_1m,
        pricing.output_usd_per_1m,
    ]
    .iter()
    .all(|rate| rate.is_finite() && *rate >= 0.0)
    {
        return None;
    }
    let mut total: u64 = 0;
    for row in usage {
        let cache_hit_tokens = row
            .cache_read_input_tokens
            .unwrap_or(0)
            .min(row.prompt_tokens);
        let uncached_tokens = row.prompt_tokens.saturating_sub(cache_hit_tokens);
        // 1 USD per 1M tokens == 1_000 nano-USD per token.
        let row_cost = (f64::from(cache_hit_tokens) * pricing.cached_input_usd_per_1m
            + f64::from(uncached_tokens) * pricing.input_usd_per_1m
            + f64::from(row.completion_tokens) * pricing.output_usd_per_1m)
            * 1_000.0;
        if !row_cost.is_finite() || row_cost < 0.0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let rounded = row_cost.round() as u64;
        total = total.saturating_add(rounded);
    }
    Some(total)
}

/// Calculates the DeepSeek model charge using reported prompt/cache usage and the UTC tariff window.
///
/// `model` identifies the tariff, so a day without usage still reports a known
/// zero for a priced model and `None` for an unsupported one. Each row is
/// priced at the moment its request was dispatched, not when the response
/// arrived.
pub fn deepseek_daily_cost_nano_usd(model: &str, usage: &[HiveModelUsage]) -> Option<u64> {
    let tariff = deepseek_tariff(model)?;
    usage.iter().try_fold(0_u64, |total, row| {
        let peak = deepseek_peak_period(row.request_at)?;
        let cache_hit_tokens = row
            .cache_read_input_tokens
            .unwrap_or(0)
            .min(row.prompt_tokens);
        let uncached_tokens = row.prompt_tokens.saturating_sub(cache_hit_tokens);
        let cache_rate = if peak {
            tariff.cache_hit_peak
        } else {
            tariff.cache_hit_off_peak
        };
        let input_rate = if peak {
            tariff.cache_miss_peak
        } else {
            tariff.cache_miss_off_peak
        };
        let output_rate = if peak {
            tariff.output_peak
        } else {
            tariff.output_off_peak
        };
        let row_cost = u64::from(cache_hit_tokens)
            .saturating_mul(cache_rate)
            .saturating_add(u64::from(uncached_tokens).saturating_mul(input_rate))
            .saturating_add(u64::from(row.completion_tokens).saturating_mul(output_rate));
        Some(total.saturating_add(row_cost))
    })
}

/// Peak interval is weekdays 01:00вЂ“04:00 and 06:00вЂ“10:00 UTC, excluding Chinese public holidays.
fn deepseek_peak_period(at_ms: i64) -> Option<bool> {
    let at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(at_ms)?;
    use chrono::{Datelike, Timelike, Weekday};
    if !matches!(
        at.weekday(),
        Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri
    ) {
        return Some(false);
    }
    if deepseek_china_public_holiday(at.year(), at.month(), at.day())? {
        return Some(false);
    }
    let hour = at.hour();
    Some((1..4).contains(&hour) || (6..10).contains(&hour))
}

/// Official mainland-China public-holiday dates used by the verified tariff calendar.
fn deepseek_china_public_holiday(year: i32, month: u32, day: u32) -> Option<bool> {
    if year != 2026 {
        return None;
    }
    Some(matches!(
        (month, day),
        (1, 1..=3)
            | (2, 15..=23)
            | (4, 4..=6)
            | (5, 1..=5)
            | (6, 19..=21)
            | (9, 25..=27)
            | (10, 1..=7)
    ))
}

/// Derives the queen status from running work and the committed limits.
fn queen_state(total: usize, budget: &HiveBudget, observation: &HiveObservation) -> HiveQueenState {
    if total > 0 {
        return HiveQueenState {
            status: HIVE_QUEEN_ACTIVE.into(),
            name: DEFAULT_QUEEN_NAME.into(),
            reason: String::new(),
            wake_at_ms: None,
        };
    }
    if budget.history_available && budget.exhausted() {
        let reset_moments = [
            if budget.minute_exhausted {
                dispatch_window_reset(observation.oldest_minute_dispatch_ms, HIVE_MINUTE_WINDOW_MS)
                    .map(|at| (at, "provider_minute_cap_reached"))
            } else {
                None
            },
            if budget.hour_exhausted {
                dispatch_window_reset(observation.oldest_hour_dispatch_ms, HIVE_HOUR_WINDOW_MS)
                    .map(|at| (at, "provider_hour_cap_reached"))
            } else {
                None
            },
            if budget.four_hour_exhausted {
                dispatch_window_reset(
                    observation.oldest_4_hours_dispatch_ms,
                    HIVE_FOUR_HOUR_WINDOW_MS,
                )
                .map(|at| (at, "provider_4h_cap_reached"))
            } else {
                None
            },
            if budget.day_exhausted {
                dispatch_window_reset(observation.oldest_day_dispatch_ms, HIVE_DAY_WINDOW_MS)
                    .map(|at| (at, "provider_day_cap_reached"))
            } else {
                None
            },
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        // All configured windows must have a free slot before dispatch can
        // proceed. Therefore the first usable moment is the latest reset among
        // every currently-exhausted window, not the earliest one.
        let next_slot = reset_moments.into_iter().max_by_key(|(at, _)| *at);
        let reason = next_slot.map_or_else(
            || {
                // The cap is known even when the oldest dispatch is not: report
                // the narrowest exhausted window instead of a vague status.
                if budget.minute_exhausted {
                    "provider_minute_cap_reached"
                } else if budget.hour_exhausted {
                    "provider_hour_cap_reached"
                } else if budget.four_hour_exhausted {
                    "provider_4h_cap_reached"
                } else {
                    "provider_day_cap_reached"
                }
            },
            |(_, reason)| reason,
        );
        return HiveQueenState {
            status: HIVE_QUEEN_BUDGET_EXHAUSTED.into(),
            name: DEFAULT_QUEEN_NAME.into(),
            reason: reason.into(),
            wake_at_ms: next_slot.map(|(at, _)| at),
        };
    }
    HiveQueenState {
        status: HIVE_QUEEN_IDLE.into(),
        name: DEFAULT_QUEEN_NAME.into(),
        reason: String::new(),
        wake_at_ms: None,
    }
}

/// Computes the first millisecond at which the oldest counted dispatch leaves a
/// rolling window.
///
/// The provenance windows are inclusive (`dispatch_at >= now - span`), so the
/// slot frees one millisecond after the oldest dispatch plus the window length.
fn dispatch_window_reset(oldest_ms: Option<i64>, window_ms: i64) -> Option<i64> {
    oldest_ms.and_then(|at| at.checked_add(window_ms)?.checked_add(1))
}

fn bounded_u32(value: u64) -> u32 {
    value.min(u64::from(u32::MAX)) as u32
}

fn limits_for(settings: &HiveSettings) -> HiveLimits {
    HiveLimits {
        max_subagents_per_worker: bounded_u16(settings.max_subagents_per_worker as usize),
        max_parallel_subagents: bounded_u16(settings.max_parallel_subagents as usize),
    }
}

/// Returns true while the projection still represents working state.
fn is_active(projection: &MinimalChildWorkflowPayload) -> bool {
    if projection.dead_letter {
        return false;
    }
    !matches!(
        projection.state,
        HiveChildState::Accepted
            | HiveChildState::Rejected
            | HiveChildState::Failed
            | HiveChildState::Cancelled
            | HiveChildState::TimedOut
            | HiveChildState::Aborted
    )
}

fn current_action(state: HiveChildState) -> String {
    match state {
        HiveChildState::Created | HiveChildState::Queued => "queued",
        HiveChildState::Running => "executing",
        HiveChildState::Validating => "validating report",
        HiveChildState::WaitingParentAcceptance => "waiting for parent",
        HiveChildState::RevisePlan => "revising plan",
        HiveChildState::Accepted => "completed",
        HiveChildState::Rejected => "rejected",
        HiveChildState::Failed => "failed",
        HiveChildState::Cancelled => "cancelled",
        HiveChildState::TimedOut => "timed out",
        HiveChildState::Aborted => "aborted",
    }
    .to_owned()
}

fn role_is_analysis(role: &str) -> bool {
    let role = role.to_ascii_lowercase();
    role.contains("research") || role.contains("analys") || role.contains("review")
}

fn role_is_planning(role: &str) -> bool {
    let role = role.to_ascii_lowercase();
    role.contains("planner") || role.contains("architect")
}

fn efficiency_percent(spawned: u64, ceiling: u64) -> u8 {
    if ceiling == 0 {
        return 0;
    }
    ((spawned.saturating_mul(100) / ceiling).min(100)) as u8
}

fn bounded_id(value: &str) -> String {
    value.chars().take(128).collect()
}

fn fallback_russian_agent_name(role: &str, child_task_id: &str) -> String {
    const NAMES: [&str; 12] = [
        "РђСЂС‚С‘Рј",
        "РђР»РёСЃР°",
        "РњР°СЂРє",
        "Р’РµСЂР°",
        "Р”РµРЅРёСЃ",
        "РљРёСЂР°",
        "РћР»РµРі",
        "РќРёРєР°",
        "РРІР°РЅ",
        "Р›РёР·Р°",
        "Р“Р»РµР±",
        "РЎРѕРЅСЏ",
    ];
    let role_ru = match role {
        "researcher" => "РСЃСЃР»РµРґРѕРІР°С‚РµР»СЊ",
        "planner" => "РџР»Р°РЅРёСЂРѕРІС‰РёРє",
        "architect" => "РђСЂС…РёС‚РµРєС‚РѕСЂ",
        "implementer" => "Р Р°Р·СЂР°Р±РѕС‚С‡РёРє",
        "tester" => "РўРµСЃС‚РёСЂРѕРІС‰РёРє",
        "reviewer" => "Р РµРІСЊСЋРµСЂ",
        "releaser" => "Р РµР»РёР·-РёРЅР¶РµРЅРµСЂ",
        other if !other.trim().is_empty() => other.trim(),
        _ => "РђРіРµРЅС‚",
    };
    let hash = child_task_id.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(usize::from(byte))
    });
    let person = NAMES.get(hash % NAMES.len()).copied().unwrap_or("РЈР»СЊСЏ");
    bounded_id(&format!("{role_ru} {person}"))
}

fn bounded_u16(value: usize) -> u16 {
    value.min(u16::MAX as usize) as u16
}

fn bounded_u8(value: usize) -> u8 {
    value.min(u8::MAX as usize) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    fn projection(id: &str, role: &str, state: HiveChildState) -> MinimalChildWorkflowPayload {
        MinimalChildWorkflowPayload {
            parent_task_id: "queen".into(),
            child_task_id: id.into(),
            role: role.into(),
            display_name: None,
            state,
            dead_letter: false,
            depth: 1,
            spawned_subagents: 2,
            planned_ceiling: 4,
        }
    }

    #[test]
    fn deepseek_flash_cost_uses_cached_input_and_utc_peak_rate() {
        let usage = [HiveModelUsage {
            prompt_tokens: 100,
            completion_tokens: 20,
            cache_read_input_tokens: Some(50),
            request_at: chrono::DateTime::parse_from_rfc3339("2026-11-16T02:00:00Z")
                .expect("valid timestamp")
                .timestamp_millis(),
        }];
        assert_eq!(
            deepseek_daily_cost_nano_usd("deepseek-flash", &usage),
            Some(39_300)
        );
    }

    #[test]
    fn deepseek_cost_is_calculated_at_request_time_not_later_response_time() {
        let usage = [HiveModelUsage {
            prompt_tokens: 100,
            completion_tokens: 20,
            cache_read_input_tokens: Some(50),
            request_at: chrono::DateTime::parse_from_rfc3339("2026-11-16T02:00:00Z")
                .expect("valid timestamp")
                .timestamp_millis(),
        }];
        assert_eq!(
            deepseek_daily_cost_nano_usd("deepseek-flash", &usage),
            Some(39_300)
        );
    }

    #[test]
    fn deepseek_v4_pro_cost_uses_off_peak_rates_on_weekends() {
        let usage = [HiveModelUsage {
            prompt_tokens: 100,
            completion_tokens: 20,
            cache_read_input_tokens: Some(50),
            request_at: chrono::DateTime::parse_from_rfc3339("2026-11-15T02:00:00Z")
                .expect("valid timestamp")
                .timestamp_millis(),
        }];
        assert_eq!(
            deepseek_daily_cost_nano_usd("deepseek-v4-pro", &usage),
            Some(73_700)
        );
    }

    #[test]
    fn deepseek_pricing_does_not_guess_for_a_model_outside_the_published_tariff() {
        assert_eq!(deepseek_daily_cost_nano_usd("other-model", &[]), None);
    }

    #[test]
    fn deepseek_pricing_reports_zero_for_a_successfully_read_empty_day() {
        assert_eq!(deepseek_daily_cost_nano_usd("deepseek-flash", &[]), Some(0));
    }

    #[test]
    fn retired_flash_spellings_are_billed_at_the_flash_rate() {
        let usage = [HiveModelUsage {
            prompt_tokens: 100,
            completion_tokens: 20,
            cache_read_input_tokens: Some(50),
            request_at: chrono::DateTime::parse_from_rfc3339("2026-11-16T02:00:00Z")
                .expect("valid timestamp")
                .timestamp_millis(),
        }];
        assert_eq!(
            deepseek_daily_cost_nano_usd("deepseek-v4-flash", &usage),
            Some(39_300)
        );
        assert_eq!(
            deepseek_daily_cost_nano_usd("deepseek-v4-flash-vision-exp", &usage),
            Some(39_300)
        );
    }

    #[test]
    fn deepseek_cache_hits_cannot_exceed_reported_prompt_tokens() {
        let usage = [HiveModelUsage {
            prompt_tokens: 10,
            completion_tokens: 0,
            cache_read_input_tokens: Some(50),
            request_at: chrono::DateTime::parse_from_rfc3339("2026-11-16T02:00:00Z")
                .expect("valid timestamp")
                .timestamp_millis(),
        }];
        assert_eq!(
            deepseek_daily_cost_nano_usd("deepseek-flash", &usage),
            Some(60)
        );
    }

    #[test]
    fn deepseek_peak_rate_excludes_public_holidays_and_the_boundary_hour() {
        let holiday = chrono::DateTime::parse_from_rfc3339("2026-01-02T02:00:00Z")
            .expect("valid timestamp")
            .timestamp_millis();
        let peak_end = chrono::DateTime::parse_from_rfc3339("2026-11-16T04:00:00Z")
            .expect("valid timestamp")
            .timestamp_millis();
        let peak_start = chrono::DateTime::parse_from_rfc3339("2026-11-16T06:00:00Z")
            .expect("valid timestamp")
            .timestamp_millis();
        assert_eq!(deepseek_peak_period(holiday), Some(false));
        assert_eq!(deepseek_peak_period(peak_end), Some(false));
        assert_eq!(deepseek_peak_period(peak_start), Some(true));
    }

    #[test]
    fn hive_budget_carries_the_daily_cost_projection_from_core_observation() {
        let observation = HiveObservation {
            daily_cost_available: true,
            daily_cost_nano_usd: Some(39_300),
            ..HiveObservation::default()
        };
        let snapshot =
            HiveStateSnapshot::from_observation(&HiveSettings::default(), &[], &[], &observation);
        assert!(snapshot.budget.daily_cost_available);
        assert_eq!(snapshot.budget.daily_cost_nano_usd, Some(39_300));
        assert_eq!(snapshot.daily_budget_usd, 0.50);
    }

    #[test]
    fn custom_pricing_calculates_daily_cost_from_usd_per_1m_rates() {
        let pricing = evohime_model_gateway::config::ModelPricingConfig {
            input_usd_per_1m: 1.50,
            cached_input_usd_per_1m: 0.15,
            output_usd_per_1m: 6.00,
        };
        let usage = [HiveModelUsage {
            prompt_tokens: 1_000,
            completion_tokens: 500,
            cache_read_input_tokens: Some(400),
            request_at: 1000,
        }];
        // 400 * 0.15 * 1000 = 60_000; 600 * 1.50 * 1000 = 900_000; 500 * 6.00 * 1000 = 3_000_000
        assert_eq!(
            custom_daily_cost_nano_usd(&pricing, &usage),
            Some(3_960_000)
        );
    }

    #[test]
    fn hive_state_snapshot_aggregates_active_agents_by_role() {
        let settings = HiveSettings::default();
        let projections = vec![
            projection("a", "researcher", HiveChildState::Running),
            projection("b", "researcher", HiveChildState::Running),
            projection("c", "reviewer", HiveChildState::Validating),
            projection("d", "researcher", HiveChildState::Accepted),
        ];
        let snapshot = HiveStateSnapshot::from_projection(&settings, &projections, &[]);
        assert_eq!(snapshot.metrics.total_agents_online, 3);
        assert_eq!(snapshot.metrics.active_tasks, 3);
        assert_eq!(snapshot.active_agents.len(), 3);
        assert_eq!(snapshot.departments.len(), 2);
        let researcher = snapshot
            .departments
            .iter()
            .find(|department| department.role == "researcher")
            .expect("researcher department");
        assert_eq!(researcher.active_count, 2);
        assert_eq!(researcher.max_capacity, 16);
        let reviewer = snapshot
            .departments
            .iter()
            .find(|department| department.role == "reviewer")
            .expect("reviewer department");
        assert_eq!(reviewer.active_count, 1);
        assert_eq!(snapshot.task_flow.processing, 3);
        assert_eq!(snapshot.task_flow.analyzing, 3);
        assert_eq!(snapshot.task_flow.generating_plan, 0);
    }

    #[test]
    fn hive_state_reflects_queen_and_limits() {
        let settings = HiveSettings {
            max_subagents_per_worker: 6,
            max_parallel_subagents: 3,
            ..HiveSettings::default()
        };
        let projections = vec![projection("worker", "planner", HiveChildState::Running)];
        let snapshot = HiveStateSnapshot::from_projection(&settings, &projections, &[]);
        assert_eq!(snapshot.schema_version, HIVE_STATE_SCHEMA_VERSION);
        assert_eq!(snapshot.queen.name, DEFAULT_QUEEN_NAME);
        assert_eq!(snapshot.queen.status, "active");
        assert_eq!(snapshot.limits.max_subagents_per_worker, 6);
        assert_eq!(snapshot.limits.max_parallel_subagents, 3);
        assert_eq!(snapshot.task_flow.generating_plan, 1);
        assert_eq!(snapshot.active_agents[0].ceiling, 4);
        assert_eq!(snapshot.active_agents[0].spawned_subagents, 2);
    }

    #[test]
    fn empty_registry_yields_empty_collections_without_panicking() {
        let snapshot = HiveStateSnapshot::from_projection(&HiveSettings::default(), &[], &[]);
        assert_eq!(snapshot.queen.status, "idle");
        assert_eq!(snapshot.metrics.total_agents_online, 0);
        assert_eq!(snapshot.metrics.active_tasks, 0);
        assert_eq!(snapshot.metrics.efficiency_percent, 0);
        assert!(snapshot.departments.is_empty());
        assert!(snapshot.active_agents.is_empty());
        assert_eq!(snapshot.task_flow, HiveTaskFlow::default());
        assert_eq!(snapshot.limits.max_subagents_per_worker, 16);
        assert_eq!(snapshot.limits.max_parallel_subagents, 4);
    }

    #[test]
    fn idle_registry_roles_are_visible_with_zero_load() {
        let snapshot = HiveStateSnapshot::from_projection(
            &HiveSettings::default(),
            &[],
            &["researcher".to_owned()],
        );
        assert_eq!(snapshot.departments.len(), 1);
        assert_eq!(snapshot.departments[0].role, "researcher");
        assert_eq!(snapshot.departments[0].active_count, 0);
        assert!(snapshot.active_agents.is_empty());
    }

    #[test]
    fn provider_failures_alone_do_not_claim_the_queen_is_paused() {
        // The snapshot deliberately has no last-provider-error heuristic. The
        // gateway can retry, route elsewhere, or accept more work; only known
        // configured caps are represented as a blocking status here.
        let snapshot = HiveStateSnapshot::from_observation(
            &HiveSettings::default(),
            &[],
            &[],
            &HiveObservation::default(),
        );
        assert_eq!(snapshot.queen.status, HIVE_QUEEN_IDLE);
        assert!(snapshot.queen.reason.is_empty());
        assert_eq!(snapshot.queen.wake_at_ms, None);
    }

    #[test]
    fn reached_minute_cap_reports_the_exact_reset_moment() {
        let now_ms = 50_000_000;
        let observation = HiveObservation {
            now_ms,
            history_available: true,
            requests_last_minute: 5,
            oldest_minute_dispatch_ms: Some(now_ms - 20_000),
            minute_cap: 5,
            ..HiveObservation::default()
        };
        let snapshot =
            HiveStateSnapshot::from_observation(&HiveSettings::default(), &[], &[], &observation);
        assert_eq!(snapshot.queen.status, HIVE_QUEEN_BUDGET_EXHAUSTED);
        assert_eq!(snapshot.queen.reason, "provider_minute_cap_reached");
        assert_eq!(
            snapshot.queen.wake_at_ms,
            Some(now_ms - 20_000 + HIVE_MINUTE_WINDOW_MS + 1)
        );
        assert!(snapshot.budget.minute_exhausted);
    }

    #[test]
    fn reached_hourly_cap_reports_the_exact_reset_moment() {
        let now_ms = 50_000_000;
        let observation = HiveObservation {
            now_ms,
            history_available: true,
            provider: "literouter".into(),
            model: "model".into(),
            requests_last_hour: 190,
            requests_last_day: 400,
            oldest_hour_dispatch_ms: Some(now_ms - 600_000),
            oldest_day_dispatch_ms: Some(now_ms - 1_000),
            hour_cap: 190,
            day_cap: 1_000,
            ..HiveObservation::default()
        };
        let snapshot =
            HiveStateSnapshot::from_observation(&HiveSettings::default(), &[], &[], &observation);
        assert_eq!(snapshot.queen.status, HIVE_QUEEN_BUDGET_EXHAUSTED);
        assert_eq!(snapshot.queen.reason, "provider_hour_cap_reached");
        assert_eq!(
            snapshot.queen.wake_at_ms,
            Some(now_ms - 600_000 + HIVE_HOUR_WINDOW_MS + 1)
        );
        assert_eq!(snapshot.budget.requests_last_hour, 190);
        assert_eq!(snapshot.budget.hour_cap, 190);
        assert!(snapshot.budget.hour_exhausted);
        assert!(!snapshot.budget.day_exhausted);
    }

    #[test]
    fn simultaneous_caps_report_the_last_blocking_reset() {
        let now_ms = 50_000_000;
        let observation = HiveObservation {
            now_ms,
            history_available: true,
            requests_last_minute: 5,
            requests_last_hour: 10,
            oldest_minute_dispatch_ms: Some(now_ms - 50_000),
            oldest_hour_dispatch_ms: Some(now_ms - 60_000),
            minute_cap: 5,
            hour_cap: 10,
            ..HiveObservation::default()
        };
        let snapshot =
            HiveStateSnapshot::from_observation(&HiveSettings::default(), &[], &[], &observation);
        assert_eq!(snapshot.queen.reason, "provider_hour_cap_reached");
        assert_eq!(
            snapshot.queen.wake_at_ms,
            Some(now_ms - 60_000 + HIVE_HOUR_WINDOW_MS + 1)
        );
    }

    #[test]
    fn unlimited_caps_are_never_reported_as_reached() {
        let observation = HiveObservation {
            now_ms: 1_000,
            history_available: true,
            requests_last_hour: 10_000,
            requests_last_day: 10_000,
            hour_cap: 0,
            day_cap: 0,
            ..HiveObservation::default()
        };
        let snapshot =
            HiveStateSnapshot::from_observation(&HiveSettings::default(), &[], &[], &observation);
        assert!(!snapshot.budget.exhausted());
        assert_eq!(snapshot.queen.status, HIVE_QUEEN_IDLE);
        assert_eq!(snapshot.queen.wake_at_ms, None);
    }

    #[test]
    fn minute_cap_can_be_the_first_exhausted_budget_window() {
        let observation = HiveObservation {
            history_available: true,
            requests_last_minute: 2,
            requests_last_hour: 2,
            requests_last_day: 2,
            minute_cap: 2,
            hour_cap: 10,
            day_cap: 20,
            ..HiveObservation::default()
        };
        let snapshot =
            HiveStateSnapshot::from_observation(&HiveSettings::default(), &[], &[], &observation);
        assert!(snapshot.budget.exhausted());
        assert!(snapshot.budget.minute_exhausted);
        assert!(!snapshot.budget.hour_exhausted);
        assert!(!snapshot.budget.day_exhausted);
        assert_eq!(snapshot.queen.reason, "provider_minute_cap_reached");
        assert_eq!(snapshot.queen.wake_at_ms, None);
    }

    #[test]
    fn hive_state_projects_the_daemon_observation_status() {
        let observation = HiveObservation {
            observation_status: HIVE_OBSERVATION_WAITING_FOR_LIMITS.into(),
            next_observation_at_ms: Some(42_000),
            ..HiveObservation::default()
        };
        let snapshot =
            HiveStateSnapshot::from_observation(&HiveSettings::default(), &[], &[], &observation);
        assert_eq!(
            snapshot.observation_status,
            HIVE_OBSERVATION_WAITING_FOR_LIMITS
        );
        assert_eq!(snapshot.next_wake_at_ms, Some(42_000));

        let observation = HiveObservation {
            observation_status: "unexpected".into(),
            ..HiveObservation::default()
        };
        let snapshot =
            HiveStateSnapshot::from_observation(&HiveSettings::default(), &[], &[], &observation);
        assert_eq!(snapshot.observation_status, HIVE_OBSERVATION_DISABLED);
    }

    #[test]
    fn hive_state_snapshot_round_trips_through_json_with_the_budget_contract() {
        let snapshot = HiveStateSnapshot::from_projection(&HiveSettings::default(), &[], &[]);
        let encoded = serde_json::to_vec(&snapshot).expect("snapshot serializes");
        let decoded: HiveStateSnapshot =
            serde_json::from_slice(&encoded).expect("snapshot deserializes");
        assert_eq!(decoded, snapshot);
        let value: serde_json::Value = serde_json::from_slice(&encoded).expect("valid json");
        assert_eq!(value["schema_version"], HIVE_STATE_SCHEMA_VERSION);
        assert_eq!(value["observation_status"], snapshot.observation_status);
        assert!(value.get("next_wake_at_ms").is_none());
        assert!(value["budget"]["hour_cap"].is_number());
        assert!(value["budget"]["four_hour_cap"].is_number());
        assert_eq!(value["queen"]["reason"], "");
    }

    #[test]
    fn four_hour_cap_projects_rp4h_and_exhaustion() {
        let observation = HiveObservation {
            history_available: true,
            requests_last_4_hours: 120,
            oldest_4_hours_dispatch_ms: Some(1_000_000),
            four_hour_cap: 120,
            ..HiveObservation::default()
        };
        let snapshot =
            HiveStateSnapshot::from_observation(&HiveSettings::default(), &[], &[], &observation);
        assert_eq!(snapshot.budget.requests_last_4_hours, 120);
        assert_eq!(snapshot.budget.four_hour_cap, 120);
        assert_eq!(snapshot.budget.limits.rp4h, 120);
        assert!(snapshot.budget.four_hour_exhausted);
        assert!(snapshot.budget.exhausted());
        assert_eq!(snapshot.queen.status, HIVE_QUEEN_BUDGET_EXHAUSTED);
        assert_eq!(snapshot.queen.reason, "provider_4h_cap_reached");
        assert_eq!(
            snapshot.queen.wake_at_ms,
            Some(1_000_000 + HIVE_FOUR_HOUR_WINDOW_MS + 1)
        );
    }
}
