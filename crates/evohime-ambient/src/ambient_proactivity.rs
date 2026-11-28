//! Side-effect-free автомат ограниченной проактивности (план 04.7).
//!
//! Здесь нет ни SQL, ни часов, ни файлов, ни сети: время приходит параметром,
//! персистентность счётчиков и mute-ключей живёт в ядре
//! (`evohime_local_storage::ambient_store`), а разделяемый реестр и публикация
//! событий — в `evohime-core` (`crate::ambient`). Этот модуль
//! отвечает ровно за три вещи:
//!
//! - **закрытый список эффектов.** Проактивно Ева может показать карточку и
//!   создать неисполняемое напоминание. Запуск задачи, вызов инструмента,
//!   запись файла и выход в сеть не являются настройкой — они запрещены
//!   инвариантом и не имеют варианта в [`ProactiveEffect`], допустимом без
//!   клика;
//! - **два ключа вместо одного.** [`proposal_key`] несёт округлённое время и
//!   стоит под `UNIQUE` — это дедупликация. [`mute_key`] времени не несёт —
//!   это «больше не предлагать такое». Один ключ на обе роли не работает: со
//!   временем внутри mute заглушил бы ровно одну временную корзину и молча
//!   перестал бы действовать через час, а без времени `UNIQUE` запретил бы
//!   любое повторное предложение по той же теме после истечения предыдущего;
//! - **скользящие счётчики потолка.** Потолок [`ProactivityBudget`] неизменяем
//!   (контракт 04.1), а текущее окно живёт здесь и персистится строкой v26.
//!   Превышение — это **отброс** со счётчиком, а не очередь: иначе после часа
//!   тишины пользователь получил бы десять карточек разом.

pub use evohime_listener_contract::{
    authorize_proactive, effect_of, mute_key, proposal_key, subject_key, AuthorizedEffect,
    ProactiveEffect, ProposalRejection, RollingCounters, MAX_SUBJECT_KEY_BYTES, PROPOSAL_BUCKET_MS,
    PROPOSAL_LIFETIME_MS,
};
use evohime_listener_contract::{
    AmbientPolicy, ContractError, ProactivityBudget, ProposalKind, ProposalState, SubjectKey,
};

#[cfg(test)]
const HOUR_MS: u64 = 60 * 60 * 1000;
#[cfg(test)]
use evohime_listener_contract::ProactivityDenial;

/// Одно предложение в терминах автомата.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// Stable identifier assigned by Core.
    pub proposal_id: String,
    /// Deduplication key with the rounded time bucket.
    pub proposal_key: String,
    /// Key used to mute this subject after a user choice.
    pub mute_key: String,
    /// Kind of action being proposed.
    pub kind: ProposalKind,
    /// Typed identity of the proposal subject.
    pub subject_key: SubjectKey,
    /// Эпизод-источник. `None` означает, что источник уже удалён.
    pub source_episode: Option<String>,
    /// Proposal creation time in Unix milliseconds.
    pub created_at_ms: u64,
    /// Current state of the proposal automaton.
    pub state: ProposalState,
}

impl Proposal {
    /// Новое предложение всегда рождается в `Proposed`: «сразу принято»
    /// означало бы действие без клика.
    pub fn new(
        proposal_id: impl Into<String>,
        kind: ProposalKind,
        subject: &str,
        source_episode: Option<String>,
        created_at_ms: u64,
    ) -> Self {
        let subject_key = subject_key(subject);
        Self {
            proposal_id: proposal_id.into(),
            proposal_key: proposal_key(kind, &subject_key, created_at_ms),
            mute_key: mute_key(kind, &subject_key),
            kind,
            subject_key,
            source_episode,
            created_at_ms,
            state: ProposalState::Proposed,
        }
    }

    /// Момент, после которого молчание пользователя считается ответом «нет».
    pub fn expires_at_ms(&self) -> u64 {
        self.created_at_ms.saturating_add(PROPOSAL_LIFETIME_MS)
    }

    /// Истекло ли молчание: предложение ещё не решено, а срок жизни вышел.
    pub fn is_expired_at(&self, now_ms: u64) -> bool {
        self.state == ProposalState::Proposed && now_ms >= self.expires_at_ms()
    }

    /// Единственный переход автомата.
    ///
    /// Из терминального состояния переходов нет: второй клик по уже решённой
    /// карточке отвечает «уже решено», а не переигрывает решение.
    pub fn transition(&mut self, next: ProposalState) -> Result<(), ProposalTransitionError> {
        if !next.is_terminal() {
            return Err(ProposalTransitionError::NotTerminal(next));
        }
        if self.state.is_terminal() {
            return Err(ProposalTransitionError::AlreadyResolved(self.state));
        }
        self.state = next;
        Ok(())
    }
}

/// Ошибка недопустимого перехода автомата предложения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalTransitionError {
    /// Предложение уже решено; повторное решение запрещено.
    AlreadyResolved(ProposalState),
    /// Из указанного состояния допустим только переход в терминальное.
    NotTerminal(ProposalState),
}

impl std::fmt::Display for ProposalTransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProposalTransitionError::AlreadyResolved(state) => {
                write!(f, "proposal is already {}", state.as_str())
            }
            ProposalTransitionError::NotTerminal(state) => {
                write!(f, "{} is not a terminal proposal state", state.as_str())
            }
        }
    }
}

impl std::error::Error for ProposalTransitionError {}

/// Полное решение «предлагать ли сейчас».
///
/// Порядок проверок — часть контракта: сперва закрытый список эффектов, потом
/// mute, и только затем политика и потолки. Заглушённая тема не должна
/// расходовать бюджет, а запрещённый эффект не должен доходить до вопроса о
/// бюджете вообще.
pub fn decide_proposal(
    effect: ProactiveEffect,
    muted: bool,
    policy: &AmbientPolicy,
    budget: &ProactivityBudget,
    counters: RollingCounters,
    now_ms: u64,
    minute_of_day: u32,
) -> Result<AuthorizedEffect, ProposalRejection> {
    let authorized = authorize_proactive(effect)?;
    if muted {
        return Err(ProposalRejection::Muted);
    }
    budget
        .decide(
            policy,
            counters.rolled(now_ms).snapshot(),
            now_ms,
            minute_of_day,
        )
        .map_err(ProposalRejection::Denied)?;
    Ok(authorized)
}

/// Проверяет потолок перед сохранением. Отдельная функция нужна ради тестов:
/// потолок обязан быть доказуем без политики и часов.
pub fn validate_budget(budget: &ProactivityBudget) -> Result<(), ContractError> {
    budget.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_listener_contract::QuietHours;

    const NOW_MS: u64 = 1_770_000_000_000;

    fn open_policy() -> AmbientPolicy {
        AmbientPolicy {
            paused: false,
            ..AmbientPolicy::default()
        }
    }

    /// Потолок соблюдается на детерминированных часах: три предложения в час
    /// и десять в сутки, не чаще одного в десять минут.
    #[test]
    fn the_hourly_and_daily_ceilings_hold_on_a_deterministic_clock() {
        let budget = ProactivityBudget::DEFAULT;
        let policy = open_policy();
        let mut counters = RollingCounters::default().rolled(NOW_MS);
        let mut shown = 0;
        let mut now = NOW_MS;
        // Полтора часа с шагом в десять минут: девять попыток.
        for _ in 0..9 {
            if decide_proposal(
                ProactiveEffect::PendingReminder,
                false,
                &policy,
                &budget,
                counters,
                now,
                12 * 60,
            )
            .is_ok()
            {
                counters = counters.record(now);
                shown += 1;
            }
            now += 10 * 60 * 1000;
        }
        assert_eq!(shown, 6, "три предложения за каждый из двух часов");

        // Сутки: потолок в десять держится даже за 24 часа попыток.
        let mut counters = RollingCounters::default().rolled(NOW_MS);
        let mut shown = 0;
        let mut now = NOW_MS;
        for _ in 0..(24 * 6) {
            if decide_proposal(
                ProactiveEffect::PendingReminder,
                false,
                &policy,
                &budget,
                counters,
                now,
                12 * 60,
            )
            .is_ok()
            {
                counters = counters.record(now);
                shown += 1;
            }
            now += 10 * 60 * 1000;
        }
        assert_eq!(shown, 10, "суточный потолок — десять");
    }

    /// Превышение потолка отбрасывает предложение, а не копит его: после часа
    /// тишины пользователь не получает десять карточек разом.
    #[test]
    fn an_over_the_cap_proposal_is_discarded_rather_than_queued() {
        let budget = ProactivityBudget::DEFAULT;
        let policy = open_policy();
        let mut counters = RollingCounters::default().rolled(NOW_MS);
        // Три подряд в пределах интервала: только первое проходит.
        for offset in [0, 60_000, 120_000] {
            let outcome = decide_proposal(
                ProactiveEffect::ProposalCard,
                false,
                &policy,
                &budget,
                counters,
                NOW_MS + offset,
                12 * 60,
            );
            if offset == 0 {
                assert!(outcome.is_ok());
                counters = counters.record(NOW_MS);
            } else {
                assert!(matches!(
                    outcome,
                    Err(ProposalRejection::Denied(ProactivityDenial::TooSoon { .. }))
                ));
            }
        }
        // Через час счётчик обнулился ровно до нуля, а не до трёх «долгов».
        let rolled = counters.rolled(NOW_MS + HOUR_MS);
        assert_eq!(rolled.hour_count, 0);
        assert_eq!(rolled.day_count, 1, "суточное окно час не обнуляет");
    }

    /// Ни один эффект вне закрытого списка не проходит авторизацию — ни при
    /// пустых счётчиках, ни при разрешающей политике.
    #[test]
    fn effects_outside_the_closed_list_are_refused_before_any_effect() {
        let budget = ProactivityBudget::DEFAULT;
        let policy = open_policy();
        for effect in ProactiveEffect::ALL {
            let outcome = decide_proposal(
                effect,
                false,
                &policy,
                &budget,
                RollingCounters::default().rolled(NOW_MS),
                NOW_MS,
                12 * 60,
            );
            if effect.is_proactively_allowed() {
                assert_eq!(outcome.map(AuthorizedEffect::effect), Ok(effect));
            } else {
                assert_eq!(
                    outcome,
                    Err(ProposalRejection::Forbidden(effect)),
                    "{} обязан быть отклонён до эффекта",
                    effect.as_str()
                );
            }
        }
        assert_eq!(
            ProactiveEffect::ALL
                .into_iter()
                .filter(|effect| effect.is_proactively_allowed())
                .count(),
            2,
            "разрешённых эффектов ровно два: карточка и напоминание"
        );
    }

    /// Заглушённая тема не расходует бюджет: mute проверяется раньше потолков.
    #[test]
    fn a_muted_subject_is_refused_before_the_budget_is_spent() {
        assert_eq!(
            decide_proposal(
                ProactiveEffect::PendingReminder,
                true,
                &open_policy(),
                &ProactivityBudget::DEFAULT,
                RollingCounters::default().rolled(NOW_MS),
                NOW_MS,
                12 * 60,
            ),
            Err(ProposalRejection::Muted)
        );
    }

    /// Пауза и тихие часы закрывают проактивность целиком.
    #[test]
    fn pause_and_quiet_hours_close_proactivity() {
        let budget = ProactivityBudget::DEFAULT;
        let paused = AmbientPolicy {
            paused: true,
            ..AmbientPolicy::default()
        };
        assert_eq!(
            decide_proposal(
                ProactiveEffect::ProposalCard,
                false,
                &paused,
                &budget,
                RollingCounters::default(),
                NOW_MS,
                12 * 60,
            ),
            Err(ProposalRejection::Denied(ProactivityDenial::Paused))
        );
        let quiet = AmbientPolicy {
            paused: false,
            quiet_hours: vec![QuietHours::new(23 * 60, 7 * 60).unwrap()],
            ..AmbientPolicy::default()
        };
        assert_eq!(
            decide_proposal(
                ProactiveEffect::ProposalCard,
                false,
                &quiet,
                &budget,
                RollingCounters::default(),
                NOW_MS,
                2 * 60,
            ),
            Err(ProposalRejection::Denied(ProactivityDenial::QuietHours))
        );
    }

    /// Ключ дедупликации меняется со временем, ключ mute — нет. Это и есть
    /// причина, по которой ключей два.
    #[test]
    fn the_dedup_key_moves_with_time_while_the_mute_key_does_not() {
        let subject = subject_key("хлеб");
        let early = proposal_key(ProposalKind::Reminder, &subject, NOW_MS);
        let later = proposal_key(ProposalKind::Reminder, &subject, NOW_MS + 2 * HOUR_MS);
        assert_ne!(early, later, "новая корзина — новый ключ дедупликации");
        assert_eq!(
            mute_key(ProposalKind::Reminder, &subject),
            mute_key(ProposalKind::Reminder, &subject)
        );
        assert_ne!(
            mute_key(ProposalKind::Reminder, &subject),
            mute_key(ProposalKind::Suggestion, &subject),
            "вид предложения входит в оба ключа"
        );
        // Один и тот же час — один и тот же ключ.
        assert_eq!(
            proposal_key(ProposalKind::Reminder, &subject, NOW_MS),
            proposal_key(ProposalKind::Reminder, &subject, NOW_MS + 60_000)
        );
    }

    /// Ключ темы — токен, а не фраза: пробелов в нём нет ни при каком вводе.
    #[test]
    fn the_subject_key_is_a_token_and_never_a_phrase() {
        assert_eq!(subject_key("Buy bread today").as_str(), "buy-bread-today");
        assert_eq!(subject_key("  Zoom  ").as_str(), "zoom");
        let cyrillic = subject_key("купить хлеб");
        assert_eq!(cyrillic.as_str().len(), 16, "кириллица даёт отпечаток");
        assert_eq!(subject_key("купить хлеб"), cyrillic, "отпечаток стабилен");
        for raw in [
            "купить хлеб",
            "Buy bread today",
            "",
            "пароль от банка 1234",
            &"a".repeat(500),
        ] {
            let key = subject_key(raw);
            assert!(!key.as_str().is_empty());
            assert!(key.as_str().len() <= MAX_SUBJECT_KEY_BYTES + 1);
            assert!(
                !key.as_str().contains(' '),
                "через ключ темы нельзя протащить фразу"
            );
        }
    }

    /// Молчание сутки — это ответ «нет», а не вечное ожидание.
    #[test]
    fn an_unanswered_proposal_expires_after_a_day() {
        let mut proposal = Proposal::new(
            "p-1",
            ProposalKind::Reminder,
            "хлеб",
            Some("ep-1".to_owned()),
            NOW_MS,
        );
        assert_eq!(proposal.state, ProposalState::Proposed);
        assert!(!proposal.is_expired_at(NOW_MS + PROPOSAL_LIFETIME_MS - 1));
        assert!(proposal.is_expired_at(NOW_MS + PROPOSAL_LIFETIME_MS));
        proposal.transition(ProposalState::Expired).unwrap();
        assert!(
            !proposal.is_expired_at(NOW_MS + 10 * PROPOSAL_LIFETIME_MS),
            "истёкшее не истекает второй раз"
        );
    }

    /// Решённая карточка не переигрывается: двойной клик не превращает отказ
    /// в согласие.
    #[test]
    fn a_resolved_proposal_refuses_a_second_decision() {
        for first in [
            ProposalState::Accepted,
            ProposalState::Declined,
            ProposalState::Muted,
            ProposalState::Expired,
        ] {
            let mut proposal = Proposal::new("p-1", ProposalKind::Suggestion, "хлеб", None, NOW_MS);
            proposal.transition(first).unwrap();
            assert_eq!(
                proposal.transition(ProposalState::Accepted),
                Err(ProposalTransitionError::AlreadyResolved(first))
            );
            assert_eq!(proposal.state, first);
        }
        let mut proposal = Proposal::new("p-1", ProposalKind::Suggestion, "хлеб", None, NOW_MS);
        assert_eq!(
            proposal.transition(ProposalState::Proposed),
            Err(ProposalTransitionError::NotTerminal(
                ProposalState::Proposed
            ))
        );
    }

    #[test]
    fn the_documented_budget_is_the_one_that_validates() {
        assert_eq!(validate_budget(&ProactivityBudget::DEFAULT), Ok(()));
        assert_eq!(
            effect_of(ProposalKind::Reminder),
            ProactiveEffect::PendingReminder
        );
        assert_eq!(
            effect_of(ProposalKind::Suggestion),
            ProactiveEffect::ProposalCard
        );
    }
}
