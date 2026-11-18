//! Cost estimation in micro-dollars (1e-6 USD). Prices are per million
//! tokens. Anthropic list prices are built in; any other model must be priced
//! through configuration (unknown models are reported as unpriced, never
//! guessed).

use std::collections::HashMap;

use super::types::LlmUsage;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    /// Defaults to 10% of the input price.
    pub cache_read_per_mtok: Option<f64>,
    /// Defaults to 125% of the input price (5-minute cache writes).
    pub cache_write_per_mtok: Option<f64>,
    /// The prices of a request whose prompt is longer, if the model has a
    /// second tier (Claude Haiku 5.5).
    pub long_prompt: Option<LongPromptPrice>,
}

/// A second tier: a request whose prompt (every input token, cached or
/// not) is over `above_tokens` is priced at `price`, all of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongPromptPrice {
    pub above_tokens: u64,
    pub price: TierPrice,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TierPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_read_per_mtok: f64,
    pub cache_write_per_mtok: f64,
}

const fn price(input: f64, output: f64, cache_read: Option<f64>) -> ModelPrice {
    ModelPrice {
        input_per_mtok: input,
        output_per_mtok: output,
        cache_read_per_mtok: cache_read,
        cache_write_per_mtok: None,
        long_prompt: None,
    }
}

/// Claude Haiku 5.5: its prompts over 100,000 tokens cost five times more.
const HAIKU_5_5: ModelPrice = ModelPrice {
    long_prompt: Some(LongPromptPrice {
        above_tokens: 100_000,
        price: TierPrice {
            input_per_mtok: 0.5,
            output_per_mtok: 2.5,
            cache_read_per_mtok: 0.05,
            cache_write_per_mtok: 0.625,
        },
    }),
    ..price(0.1, 0.5, None)
};

/// Anthropic's list prices, read on 2026-11-17 at
/// https://platform.claude.com/docs/en/about-claude/pricing (standard
/// routing, 5-minute cache writes).
pub const ANTHROPIC_PRICES: &[(&str, ModelPrice)] = &[
    ("claude-fable-5-2", price(8.0, 40.0, Some(0.2))),
    ("claude-mythos-5-2", price(8.0, 40.0, Some(0.2))),
    ("claude-fable-5-1", price(10.0, 50.0, Some(0.25))),
    ("claude-mythos-5-1", price(10.0, 50.0, Some(0.25))),
    ("claude-fable-5", price(10.0, 50.0, None)),
    ("claude-mythos-5", price(10.0, 50.0, None)),
    ("claude-opus-5-5", price(4.0, 20.0, Some(0.2))),
    ("claude-opus-5", price(5.0, 25.0, None)),
    ("claude-opus-4-8", price(5.0, 25.0, None)),
    ("claude-opus-4-7", price(5.0, 25.0, None)),
    ("claude-opus-4-6", price(5.0, 25.0, None)),
    ("claude-opus-4-5", price(5.0, 25.0, None)),
    ("claude-sonnet-5-5", price(2.0, 10.0, Some(0.1))),
    ("claude-sonnet-5", price(2.0, 10.0, None)),
    ("claude-sonnet-4-6", price(3.0, 15.0, None)),
    ("claude-haiku-5-5", HAIKU_5_5),
    ("claude-haiku-4-5", price(1.0, 5.0, None)),
];

#[derive(Debug, Clone)]
pub struct PriceTable {
    prices: HashMap<String, ModelPrice>,
}

impl Default for PriceTable {
    fn default() -> Self {
        Self::new(HashMap::new())
    }
}

impl PriceTable {
    /// Built-in prices, overridden or extended by `extra`.
    pub fn new(extra: HashMap<String, ModelPrice>) -> Self {
        let mut prices: HashMap<String, ModelPrice> = ANTHROPIC_PRICES
            .iter()
            .map(|(model, p)| ((*model).to_string(), *p))
            .collect();
        prices.extend(extra);
        Self { prices }
    }

    pub fn has(&self, model: &str) -> bool {
        self.price_of(model).is_some()
    }

    /// Estimated cost in micro-dollars, or `None` when the model is not priced.
    pub fn cost_micros(&self, model: &str, usage: &LlmUsage) -> Option<i64> {
        let tier = self.price_of(model)?.tier_for(usage.input_total());
        // $/MTok equals micro-dollars per token.
        let micros = usage.input_tokens as f64 * tier.input_per_mtok
            + usage.output_tokens as f64 * tier.output_per_mtok
            + usage.cache_read_tokens as f64 * tier.cache_read_per_mtok
            + usage.cache_write_tokens as f64 * tier.cache_write_per_mtok;
        Some(micros.round() as i64)
    }
}

impl PriceTable {
    /// The price of `model`, or of its alias when it names a dated snapshot
    /// (`claude-opus-4-5-20251101` is `claude-opus-4-5`).
    fn price_of(&self, model: &str) -> Option<&ModelPrice> {
        self.prices
            .get(model)
            .or_else(|| self.prices.get(undated(model)?))
    }
}

/// The alias of a dated model identifier: its name before an eight-digit
/// date.
fn undated(model: &str) -> Option<&str> {
    let (alias, date) = model.rsplit_once('-')?;
    (date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit())).then_some(alias)
}

impl ModelPrice {
    /// The prices of a request whose prompt is `prompt_tokens` long.
    fn tier_for(&self, prompt_tokens: u64) -> TierPrice {
        match self.long_prompt {
            Some(long) if prompt_tokens > long.above_tokens => long.price,
            _ => TierPrice {
                input_per_mtok: self.input_per_mtok,
                output_per_mtok: self.output_per_mtok,
                cache_read_per_mtok: self
                    .cache_read_per_mtok
                    .unwrap_or(self.input_per_mtok * 0.1),
                cache_write_per_mtok: self
                    .cache_write_per_mtok
                    .unwrap_or(self.input_per_mtok * 1.25),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u64, output: u64, read: u64, write: u64) -> LlmUsage {
        LlmUsage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: read,
            cache_write_tokens: write,
        }
    }

    fn cost(model: &str, usage: LlmUsage) -> Option<i64> {
        PriceTable::default().cost_micros(model, &usage)
    }

    #[test]
    fn a_haiku_5_5_prompt_over_100_000_tokens_is_priced_whole_at_the_higher_tier() {
        // At the threshold: the first tier, cache reads at 10%.
        let at = usage(60_000, 1_000, 40_000, 0);
        assert_eq!(cost("claude-haiku-5-5", at), Some(6_000 + 500 + 400));
        // One token over, counting cache reads and writes: all five times
        // higher, output included.
        let over = usage(60_000, 1_000, 40_000, 1);
        assert_eq!(
            cost("claude-haiku-5-5", over),
            Some(30_000 + 2_500 + 2_000 + 1)
        );
    }

    #[test]
    fn cache_reads_follow_each_model_s_own_rate() {
        let reads = usage(0, 0, 1_000_000, 0);
        assert_eq!(cost("claude-sonnet-5-5", reads), Some(100_000));
        assert_eq!(cost("claude-opus-5-5", reads), Some(200_000));
        assert_eq!(cost("claude-fable-5-2", reads), Some(200_000));
        assert_eq!(cost("claude-opus-4-8", reads), Some(500_000));
        assert_eq!(cost("claude-fable-5", reads), Some(1_000_000));
    }

    #[test]
    fn a_dated_identifier_is_priced_as_its_alias_and_nothing_else_is_guessed() {
        let call = usage(1_000_000, 0, 0, 0);
        assert_eq!(cost("claude-opus-4-5-20251101", call), Some(5_000_000));
        assert_eq!(cost("claude-haiku-4-5-20251001", call), Some(1_000_000));
        for unpriced in [
            "claude-opus-4-5-2025110",
            "claude-opus-4-5-2025110a",
            "20251101",
        ] {
            assert_eq!(cost(unpriced, call), None, "{unpriced}");
        }
    }

    #[test]
    fn the_models_an_escalation_may_name_are_priced_and_others_are_not_guessed() {
        let call = usage(1_000_000, 1_000_000, 0, 0);
        assert_eq!(cost("claude-sonnet-4-6", call), Some(18_000_000));
        assert_eq!(cost("claude-haiku-4-5", call), Some(6_000_000));
        assert_eq!(cost("claude-mythos-5-2", call), Some(48_000_000));
        assert_eq!(cost("un-modele-inconnu", call), None);
    }
}
