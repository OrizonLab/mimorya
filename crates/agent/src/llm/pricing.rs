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
}

const fn price(input: f64, output: f64, cache_read: Option<f64>) -> ModelPrice {
    ModelPrice {
        input_per_mtok: input,
        output_per_mtok: output,
        cache_read_per_mtok: cache_read,
        cache_write_per_mtok: None,
    }
}

pub const ANTHROPIC_PRICES: &[(&str, ModelPrice)] = &[
    ("claude-fable-5-1", price(10.0, 50.0, Some(0.25))),
    ("claude-opus-5-5", price(4.0, 20.0, Some(0.2))),
    ("claude-opus-5", price(5.0, 25.0, None)),
    ("claude-sonnet-5-5", price(2.0, 10.0, Some(0.2))),
    ("claude-sonnet-5", price(2.0, 10.0, None)),
    ("claude-haiku-5-5", price(0.1, 0.5, None)),
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
        self.prices.contains_key(model)
    }

    /// Estimated cost in micro-dollars, or `None` when the model is not priced.
    pub fn cost_micros(&self, model: &str, usage: &LlmUsage) -> Option<i64> {
        let price = self.prices.get(model)?;
        let cache_read = price
            .cache_read_per_mtok
            .unwrap_or(price.input_per_mtok * 0.1);
        let cache_write = price
            .cache_write_per_mtok
            .unwrap_or(price.input_per_mtok * 1.25);
        // $/MTok equals micro-dollars per token.
        let micros = usage.input_tokens as f64 * price.input_per_mtok
            + usage.output_tokens as f64 * price.output_per_mtok
            + usage.cache_read_tokens as f64 * cache_read
            + usage.cache_write_tokens as f64 * cache_write;
        Some(micros.round() as i64)
    }
}
