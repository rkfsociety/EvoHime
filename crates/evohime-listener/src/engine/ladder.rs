//! Бюджет распознавания: измерение RTF и лестница моделей.
//!
//! RTF (real-time factor) — отношение времени распознавания к длительности
//! речи. RTF выше 0.5 означает, что на каждую секунду речи уходит больше
//! полсекунды процессора: на коротких высказываниях через VAD это ещё
//! терпимо, но подряд — уже отставание, которое само не рассосётся.

use super::ModelRung;

/// Порог, выше которого высказывание считается «дорогим».
pub const RTF_THRESHOLD: f32 = 0.5;
/// Сколько дорогих высказываний подряд переключают модель.
pub const RTF_BREACH_STREAK: u8 = 5;

/// Что делать после очередного измерения.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LadderAction {
    /// Бюджет соблюдён либо серия ещё не набралась.
    Keep,
    /// Переключиться на более лёгкую модель.
    Downgrade(ModelRung),
    /// Лестница исчерпана: слушание останавливается политикой.
    Degrade,
}

/// Состояние лестницы на одну сессию.
///
/// Обратного хода нет: улучшение нагрузки не возвращает тяжёлую модель в той
/// же сессии. Иначе на границе порога движок перезагружал бы модель туда-сюда,
/// а перезагрузка сама по себе стоит секунд.
#[derive(Debug)]
pub struct RtfLadder {
    rung: ModelRung,
    breaches: u8,
    degraded: bool,
}

impl RtfLadder {
    pub fn new(rung: ModelRung) -> Self {
        Self {
            rung,
            breaches: 0,
            degraded: false,
        }
    }

    pub const fn rung(&self) -> ModelRung {
        self.rung
    }

    pub const fn degraded(&self) -> bool {
        self.degraded
    }

    /// Текущее число дорогих высказываний подряд.
    pub const fn breaches(&self) -> u8 {
        self.breaches
    }

    /// Учитывает одно распознавание.
    ///
    /// Нулевая длительность речи не измеряется: делить на неё нельзя, а
    /// считать такой вызов «дорогим» значило бы штрафовать движок за пустой
    /// сегмент.
    pub fn observe(&mut self, speech_ms: u32, elapsed_ms: u32) -> LadderAction {
        if self.degraded || speech_ms == 0 {
            return LadderAction::Keep;
        }
        let rtf = elapsed_ms as f32 / speech_ms as f32;
        if rtf <= RTF_THRESHOLD {
            self.breaches = 0;
            return LadderAction::Keep;
        }
        self.breaches = self.breaches.saturating_add(1);
        if self.breaches < RTF_BREACH_STREAK {
            return LadderAction::Keep;
        }
        self.breaches = 0;
        match self.rung.next_lower() {
            Some(next) => {
                self.rung = next;
                LadderAction::Downgrade(next)
            }
            None => {
                self.degraded = true;
                LadderAction::Degrade
            }
        }
    }

    /// Явное переключение, когда нужной модели в поставке не оказалось.
    pub fn force(&mut self, rung: ModelRung) {
        self.rung = rung;
        self.breaches = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breach(ladder: &mut RtfLadder) -> LadderAction {
        // 1000 мс речи за 900 мс — RTF 0.9, заведомо выше порога.
        ladder.observe(1_000, 900)
    }

    #[test]
    fn a_single_slow_utterance_changes_nothing() {
        let mut ladder = RtfLadder::new(ModelRung::Small);
        assert_eq!(breach(&mut ladder), LadderAction::Keep);
        assert_eq!(ladder.rung(), ModelRung::Small);
    }

    #[test]
    fn five_slow_utterances_in_a_row_step_down() {
        let mut ladder = RtfLadder::new(ModelRung::Small);
        for _ in 0..RTF_BREACH_STREAK - 1 {
            assert_eq!(breach(&mut ladder), LadderAction::Keep);
        }
        assert_eq!(
            breach(&mut ladder),
            LadderAction::Downgrade(ModelRung::Base)
        );
        assert_eq!(ladder.rung(), ModelRung::Base);
    }

    #[test]
    fn a_fast_utterance_resets_the_streak() {
        let mut ladder = RtfLadder::new(ModelRung::Small);
        for _ in 0..RTF_BREACH_STREAK - 1 {
            breach(&mut ladder);
        }
        assert_eq!(ladder.observe(1_000, 100), LadderAction::Keep);
        assert_eq!(ladder.breaches(), 0);
        assert_eq!(breach(&mut ladder), LadderAction::Keep);
        assert_eq!(ladder.rung(), ModelRung::Small);
    }

    #[test]
    fn the_bottom_of_the_ladder_degrades_instead_of_looping() {
        let mut ladder = RtfLadder::new(ModelRung::Tiny);
        for _ in 0..RTF_BREACH_STREAK - 1 {
            breach(&mut ladder);
        }
        assert_eq!(breach(&mut ladder), LadderAction::Degrade);
        assert!(ladder.degraded());
        // После деградации измерения больше ничего не меняют.
        assert_eq!(breach(&mut ladder), LadderAction::Keep);
    }

    #[test]
    fn the_whole_ladder_is_walked_once() {
        let mut ladder = RtfLadder::new(ModelRung::Small);
        let mut actions = Vec::new();
        for _ in 0..RTF_BREACH_STREAK * 3 {
            let action = breach(&mut ladder);
            if action != LadderAction::Keep {
                actions.push(action);
            }
        }
        assert_eq!(
            actions,
            vec![
                LadderAction::Downgrade(ModelRung::Base),
                LadderAction::Downgrade(ModelRung::Tiny),
                LadderAction::Degrade,
            ]
        );
    }

    #[test]
    fn exactly_at_the_threshold_is_not_a_breach() {
        let mut ladder = RtfLadder::new(ModelRung::Small);
        for _ in 0..RTF_BREACH_STREAK * 2 {
            assert_eq!(ladder.observe(1_000, 500), LadderAction::Keep);
        }
        assert_eq!(ladder.rung(), ModelRung::Small);
    }

    #[test]
    fn zero_length_speech_is_not_measured() {
        let mut ladder = RtfLadder::new(ModelRung::Small);
        for _ in 0..RTF_BREACH_STREAK * 2 {
            assert_eq!(ladder.observe(0, 5_000), LadderAction::Keep);
        }
        assert_eq!(ladder.rung(), ModelRung::Small);
    }
}
