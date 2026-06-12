//! 模型成本估算 —— 用已知单价把 token 用量折算成美元(粗略,用于省钱路由的量化)。

/// 某模型的单价:美元 / 百万 token(输入、输出)。
struct Price {
    input_per_mtok: f64,
    output_per_mtok: f64,
}

/// 按 model 名(子串匹配)返回单价;未知模型返回零价(成本计 0)。
///
/// 匹配顺序要点:更具体的名字放前面(如 `gpt-4o-mini` 必须在 `gpt-4o` 之前匹配)。
fn price_for(model: &str) -> Price {
    let m = model.to_ascii_lowercase();
    // 顺序敏感:先匹配更具体的关键词
    if m.contains("haiku") {
        Price { input_per_mtok: 0.80, output_per_mtok: 4.00 }
    } else if m.contains("opus") {
        Price { input_per_mtok: 15.00, output_per_mtok: 75.00 }
    } else if m.contains("sonnet") {
        Price { input_per_mtok: 3.00, output_per_mtok: 15.00 }
    } else if m.contains("gpt-4o-mini") {
        Price { input_per_mtok: 0.15, output_per_mtok: 0.60 }
    } else if m.contains("gpt-4o") {
        Price { input_per_mtok: 2.50, output_per_mtok: 10.00 }
    } else {
        Price { input_per_mtok: 0.0, output_per_mtok: 0.0 }
    }
}

/// 估算一次用量的美元成本。未知模型返回 0.0。
pub fn estimate_cost(model: &str, prompt_tokens: u64, completion_tokens: u64) -> f64 {
    let p = price_for(model);
    (prompt_tokens as f64 / 1_000_000.0) * p.input_per_mtok
        + (completion_tokens as f64 / 1_000_000.0) * p.output_per_mtok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_model_cost() {
        // sonnet: 1M prompt * 3.00 + 1M completion * 15.00 = 18.00
        let c = estimate_cost("claude-sonnet-4-5", 1_000_000, 1_000_000);
        assert!((c - 18.00).abs() < 1e-9, "got {c}");
    }

    #[test]
    fn haiku_cheaper_than_sonnet() {
        let h = estimate_cost("claude-3-5-haiku-latest", 1_000_000, 1_000_000);
        let s = estimate_cost("claude-sonnet-4-5", 1_000_000, 1_000_000);
        assert!(h < s, "haiku {h} should be cheaper than sonnet {s}");
    }

    #[test]
    fn gpt4o_mini_matched_before_gpt4o() {
        // mini 单价远低于 4o;若匹配顺序错(先匹配 gpt-4o)会得到 4o 的价
        let mini = estimate_cost("gpt-4o-mini", 1_000_000, 0);
        assert!((mini - 0.15).abs() < 1e-9, "got {mini}");
    }

    #[test]
    fn unknown_model_is_zero() {
        assert_eq!(estimate_cost("some-local-model", 1_000_000, 1_000_000), 0.0);
    }
}
