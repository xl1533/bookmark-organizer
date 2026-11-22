//! #13 域名 + 关键词规则归类。
//!
//! 规则按列表顺序自上而下匹配，首个「已启用且命中」的规则胜出。
//! 本期只做精确/后缀域名匹配与大小写不敏感的包含匹配，不做正则、不做通配。
//! 规则集以 `Rule[]` JSON 形式持久化到 `settings.classify_rules`。

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::CoreResult;

/// #13 规则匹配字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleField {
    /// 域名精确或后缀匹配（按点边界）
    Host,
    /// URL 包含匹配（忽略大小写）
    UrlKeyword,
    /// 标题包含匹配（忽略大小写）
    TitleKeyword,
}

/// #13 单条归类规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub field: RuleField,
    pub pattern: String,
    /// 相对书签栏的目标目录，如 `开发/前端`
    pub target_path: String,
    pub enabled: bool,
}

impl Rule {
    /// 该规则是否命中给定书签（不检查 enabled，由调用方统一过滤）。
    pub fn matches(&self, url: &str, title: &str) -> bool {
        let needle = self.pattern.trim();
        if needle.is_empty() {
            return false;
        }
        match self.field {
            RuleField::Host => {
                let Ok(parsed) = Url::parse(url) else {
                    return false;
                };
                let Some(host) = parsed.host_str() else {
                    return false;
                };
                let host = host.trim_end_matches('.').to_ascii_lowercase();
                let pattern = needle
                    .trim_start_matches('.')
                    .trim_end_matches('.')
                    .to_ascii_lowercase();
                if pattern.is_empty() {
                    return false;
                }
                // 精确匹配或点边界后缀匹配：`example.com` 命中 `www.example.com`，
                // 但不命中 `notexample.com` / `example.com.cn`。
                host == pattern || host.ends_with(&format!(".{pattern}"))
            }
            RuleField::UrlKeyword => url.to_lowercase().contains(&needle.to_lowercase()),
            RuleField::TitleKeyword => title.to_lowercase().contains(&needle.to_lowercase()),
        }
    }
}

/// #13 新增规则入参（id 由后端生成）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInput {
    pub field: RuleField,
    pub pattern: String,
    pub target_path: String,
    pub enabled: bool,
}

impl RuleInput {
    /// 用后端生成的 id 转为完整规则。
    pub fn into_rule(self, id: String) -> Rule {
        Rule {
            id,
            field: self.field,
            pattern: self.pattern,
            target_path: self.target_path,
            enabled: self.enabled,
        }
    }
}

/// #13 有序规则集。顺序即优先级，持久化时保持数组顺序。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new(rules: Vec<Rule>) -> RuleSet {
        RuleSet { rules }
    }

    /// 2-3 条可修改的示例规则（全部相对书签栏）。
    pub fn sample() -> RuleSet {
        RuleSet::new(vec![
            Rule {
                id: "sample-github-host".to_string(),
                field: RuleField::Host,
                pattern: "github.com".to_string(),
                target_path: "开发/代码".to_string(),
                enabled: true,
            },
            Rule {
                id: "sample-title-tutorial".to_string(),
                field: RuleField::TitleKeyword,
                pattern: "教程".to_string(),
                target_path: "学习".to_string(),
                enabled: true,
            },
            Rule {
                id: "sample-url-docs".to_string(),
                field: RuleField::UrlKeyword,
                pattern: "docs".to_string(),
                target_path: "资料".to_string(),
                enabled: true,
            },
        ])
    }

    /// 从 settings 值读取规则集；缺失或内容非法时回填示例规则，避免坏配置令功能不可用。
    pub fn from_settings_json(raw: &str) -> RuleSet {
        if raw.trim().is_empty() {
            return RuleSet::sample();
        }
        match serde_json::from_str::<Vec<Rule>>(raw) {
            Ok(rules) => RuleSet::new(rules),
            Err(_) => RuleSet::sample(),
        }
    }

    /// 严格解析（调用方需要区分非法配置时使用）。
    pub fn parse_settings_json(raw: &str) -> CoreResult<RuleSet> {
        let rules: Vec<Rule> = serde_json::from_str(raw)
            .map_err(|e| crate::CoreError::Other(format!("规则配置解析失败: {e}")))?;
        Ok(RuleSet::new(rules))
    }

    /// 序列化为 `settings.classify_rules` 的值（`Rule[]`）。
    pub fn to_settings_json(&self) -> String {
        serde_json::to_string(&self.rules).unwrap_or_else(|_| "[]".to_string())
    }

    /// 自上而下返回首个已启用且命中的规则；无命中返回 None。
    pub fn first_match(&self, url: &str, title: &str) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|rule| rule.enabled && rule.matches(url, title))
    }

    /// 按传入 id 顺序重排；未列出的规则按原顺序追加在末尾，避免误删。
    pub fn reorder(&mut self, ids: &[String]) {
        let original = std::mem::take(&mut self.rules);
        let positions: HashMap<&str, usize> = original
            .iter()
            .enumerate()
            .map(|(index, rule)| (rule.id.as_str(), index))
            .collect();
        let mut used: HashSet<usize> = HashSet::new();
        let mut ordered: Vec<Rule> = Vec::with_capacity(original.len());
        for id in ids {
            if let Some(&index) = positions.get(id.as_str()) {
                if used.insert(index) {
                    ordered.push(original[index].clone());
                }
            }
        }
        // positions 持有指向 original 的借用，消费 original 前先释放。
        drop(positions);
        for (index, rule) in original.into_iter().enumerate() {
            if !used.contains(&index) {
                ordered.push(rule);
            }
        }
        self.rules = ordered;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, field: RuleField, pattern: &str, target: &str) -> Rule {
        Rule {
            id: id.to_string(),
            field,
            pattern: pattern.to_string(),
            target_path: target.to_string(),
            enabled: true,
        }
    }

    #[test]
    fn test_host_exact_suffix_and_dot_boundary() {
        let set = RuleSet::new(vec![rule("h", RuleField::Host, "example.com", "目标")]);
        assert_eq!(
            set.first_match("https://example.com/path", "").map(|r| r.id.as_str()),
            Some("h")
        );
        assert_eq!(
            set.first_match("https://www.example.com/path", "").map(|r| r.id.as_str()),
            Some("h")
        );
        // 不是点边界：notexample.com 不应命中
        assert!(set.first_match("https://notexample.com", "").is_none());
        // 后缀继续延伸：example.com.cn 不应命中
        assert!(set.first_match("https://example.com.cn", "").is_none());
        // 非 URL 不命中
        assert!(set.first_match("not a url", "").is_none());
    }

    #[test]
    fn test_keyword_contains_case_insensitive() {
        let set = RuleSet::new(vec![
            rule("u", RuleField::UrlKeyword, "Tutorial", "资料"),
            rule("t", RuleField::TitleKeyword, "教程", "学习"),
        ]);
        assert_eq!(
            set.first_match("https://site.com/TUTORIAL/intro", "首页").map(|r| r.id.as_str()),
            Some("u")
        );
        assert_eq!(
            set.first_match("https://site.com/p", "零基础教程").map(|r| r.id.as_str()),
            Some("t")
        );
        assert!(set.first_match("https://site.com/p", "无关标题").is_none());
    }

    #[test]
    fn test_first_match_wins_and_reorder() {
        let mut set = RuleSet::new(vec![
            rule("a", RuleField::UrlKeyword, "docs", "A"),
            rule("b", RuleField::Host, "example.com", "B"),
        ]);
        assert_eq!(
            set.first_match("https://example.com/docs", "").map(|r| r.target_path.as_str()),
            Some("A")
        );
        set.reorder(&["b".to_string(), "a".to_string()]);
        assert_eq!(set.rules[0].id, "b");
        assert_eq!(
            set.first_match("https://example.com/docs", "").map(|r| r.target_path.as_str()),
            Some("B")
        );
        // 禁用后落到下一条
        set.rules[0].enabled = false;
        assert_eq!(
            set.first_match("https://example.com/docs", "").map(|r| r.target_path.as_str()),
            Some("A")
        );
    }

    #[test]
    fn test_settings_json_roundtrip_and_sample() {
        let sample = RuleSet::sample();
        assert_eq!(sample.rules.len(), 3);
        let json = sample.to_settings_json();
        let parsed = RuleSet::parse_settings_json(&json).unwrap();
        assert_eq!(parsed, sample);
        assert_eq!(RuleSet::from_settings_json(""), sample);
        assert_eq!(RuleSet::from_settings_json("{bad json"), sample);
        assert_eq!(
            RuleSet::from_settings_json(&json)
                .first_match("https://github.com/foo", "")
                .map(|r| r.target_path.as_str()),
            Some("开发/代码")
        );
    }
}
