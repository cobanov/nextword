//! Context buffer. Keeps the last N chars typed and trims to sentence
//! boundaries when possible. Real implementation lands in M2/M3.

use parking_lot::Mutex;

const MAX_CHARS: usize = 256;

#[derive(Debug, Default)]
pub struct ContextBuffer {
    inner: Mutex<String>,
}

impl ContextBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_char(&self, c: char) {
        let mut buf = self.inner.lock();
        buf.push(c);
        if buf.chars().count() > MAX_CHARS {
            let trimmed: String = buf.chars().skip(buf.chars().count() - MAX_CHARS).collect();
            *buf = trimmed;
        }
    }

    pub fn push_str(&self, s: &str) {
        for c in s.chars() {
            self.push_char(c);
        }
    }

    pub fn replace(&self, s: &str) {
        let mut buf = self.inner.lock();
        let trimmed: String = s.chars().rev().take(MAX_CHARS).collect::<String>()
            .chars().rev().collect();
        *buf = trimmed;
    }

    pub fn clear(&self) {
        self.inner.lock().clear();
    }

    pub fn snapshot(&self) -> String {
        self.inner.lock().clone()
    }

    pub fn len_chars(&self) -> usize {
        self.inner.lock().chars().count()
    }
}

/// Trim the front of `s` to the closest sentence start within `max_chars`.
/// If no sentence break is found, just keep the last `max_chars`.
pub fn trim_to_sentence_start(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(max_chars);
    let slice = &chars[start..];

    for (i, w) in slice.windows(2).enumerate() {
        let prev = w[0];
        let curr = w[1];
        if matches!(prev, '.' | '!' | '?') && curr == ' ' {
            return slice[i + 2..].iter().collect();
        }
    }
    slice.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_keeps_last_256_chars() {
        let b = ContextBuffer::new();
        for _ in 0..500 {
            b.push_char('a');
        }
        assert_eq!(b.len_chars(), MAX_CHARS);
    }

    #[test]
    fn trim_to_sentence_start_finds_break() {
        let s = "Hello world. This is a test.";
        let out = trim_to_sentence_start(s, 100);
        assert_eq!(out, "This is a test.");
    }
}
