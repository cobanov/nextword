//! Trigger rules. Decides whether a keystroke should fire a prediction.

#[derive(Debug, Clone, Copy)]
pub struct TriggerInput<'a> {
    pub context: &'a str,
    pub key_is_space: bool,
    pub modifier_pressed: bool,
    pub is_secure_field: bool,
}

const MIN_CHARS: usize = 10;
const MIN_WORDS: usize = 2;

pub fn should_trigger(input: TriggerInput<'_>) -> bool {
    if !input.key_is_space {
        return false;
    }
    if input.modifier_pressed {
        return false;
    }
    if input.is_secure_field {
        return false;
    }
    if input.context.chars().count() < MIN_CHARS {
        return false;
    }
    if input.context.split_whitespace().count() < MIN_WORDS {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base<'a>(ctx: &'a str) -> TriggerInput<'a> {
        TriggerInput {
            context: ctx,
            key_is_space: true,
            modifier_pressed: false,
            is_secure_field: false,
        }
    }

    #[test]
    fn rejects_short_context() {
        assert!(!should_trigger(base("hi ")));
    }

    #[test]
    fn rejects_single_word() {
        assert!(!should_trigger(base("supercalifragilistic ")));
    }

    #[test]
    fn rejects_modifier() {
        let mut i = base("I went to the ");
        i.modifier_pressed = true;
        assert!(!should_trigger(i));
    }

    #[test]
    fn rejects_secure_field() {
        let mut i = base("I went to the ");
        i.is_secure_field = true;
        assert!(!should_trigger(i));
    }

    #[test]
    fn rejects_non_space() {
        let mut i = base("I went to the ");
        i.key_is_space = false;
        assert!(!should_trigger(i));
    }

    #[test]
    fn accepts_normal_typing() {
        assert!(should_trigger(base("I went to the ")));
    }
}
