use super::sources::{ConfigSnapshot, parse_git_bool, parse_positive_usize};
use super::{
    FileConfig, FileRedactionRules, Provider, RawConfigValues, ReasoningEffort,
    default_redaction_rules,
};
use redactor::RedactionRules;

fn parse_comma_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|part| part.trim().trim_end_matches('/').to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

impl ConfigSnapshot {
    pub(super) fn provider_value(&self) -> Result<Provider, String> {
        let raw = self
            .env
            .provider
            .as_ref()
            .or(self.git.provider.as_ref())
            .or_else(|| self.file.as_ref().and_then(|cfg| cfg.provider.as_ref()));
        match raw {
            Some(raw) => Provider::parse(raw)
                .ok_or_else(|| format!("invalid ai.commit.provider value {:?}", raw)),
            None => Ok(Provider::default()),
        }
    }

    pub(super) fn string_value(
        &self,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<&String>,
    ) -> String {
        raw_getter(&self.env)
            .cloned()
            .or_else(|| raw_getter(&self.git).cloned())
            .or_else(|| self.file.as_ref().and_then(|cfg| file_getter(cfg).cloned()))
            .unwrap_or_default()
    }

    pub(super) fn list_value(
        &self,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<&String>,
        fallback: Vec<String>,
    ) -> Vec<String> {
        let raw = raw_getter(&self.env)
            .or_else(|| raw_getter(&self.git))
            .or_else(|| self.file.as_ref().and_then(file_getter));
        match raw {
            Some(raw) => parse_comma_list(raw),
            None => fallback,
        }
    }

    pub(super) fn has_configured_value(
        &self,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<usize>,
    ) -> bool {
        raw_getter(&self.env).is_some()
            || raw_getter(&self.git).is_some()
            || self.file.as_ref().and_then(file_getter).is_some()
    }

    pub(super) fn bool_value(
        &self,
        config_key: &str,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<bool>,
        fallback: bool,
    ) -> Result<bool, String> {
        if let Some(raw) = raw_getter(&self.env).or_else(|| raw_getter(&self.git)) {
            return parse_git_bool(raw)
                .ok_or_else(|| format!("invalid {config_key} value {:?}", raw));
        }
        Ok(self.file.as_ref().and_then(file_getter).unwrap_or(fallback))
    }

    pub(super) fn int_value(
        &self,
        config_key: &str,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<usize>,
        fallback: usize,
    ) -> Result<usize, String> {
        if let Some(raw) = raw_getter(&self.env).or_else(|| raw_getter(&self.git)) {
            return parse_positive_usize(raw)
                .ok_or_else(|| format!("invalid {config_key} value {:?}", raw));
        }
        match self.file.as_ref().and_then(file_getter) {
            Some(value) if value > 0 => Ok(value),
            Some(value) => Err(format!("invalid {config_key} value {value:?}")),
            None => Ok(fallback),
        }
    }

    pub(super) fn optional_int_value(
        &self,
        config_key: &str,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<usize>,
    ) -> Result<Option<usize>, String> {
        if let Some(raw) = raw_getter(&self.env).or_else(|| raw_getter(&self.git)) {
            return parse_positive_usize(raw)
                .map(Some)
                .ok_or_else(|| format!("invalid {config_key} value {:?}", raw));
        }
        match self.file.as_ref().and_then(file_getter) {
            Some(value) if value > 0 => Ok(Some(value)),
            Some(value) => Err(format!("invalid {config_key} value {value:?}")),
            None => Ok(None),
        }
    }

    pub(super) fn reasoning_effort_value(
        &self,
        config_key: &str,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<&str>,
        fallback: ReasoningEffort,
    ) -> Result<ReasoningEffort, String> {
        let raw = raw_getter(&self.env)
            .or_else(|| raw_getter(&self.git))
            .map(String::as_str)
            .or_else(|| self.file.as_ref().and_then(file_getter));
        match raw {
            Some(raw) => ReasoningEffort::parse(raw)
                .ok_or_else(|| format!("invalid {config_key} value {:?}", raw)),
            None => Ok(fallback),
        }
    }

    pub(super) fn redaction_rules(&self) -> Result<RedactionRules, String> {
        let mut rules = default_redaction_rules();
        macro_rules! resolve {
            ($rule:ident, $raw:ident) => {
                rules.$rule = self.redaction_rule_value(
                    concat!("ai.commit.redaction.", stringify!($rule)),
                    |values| values.$raw.as_ref(),
                    |file| file.$rule,
                    rules.$rule,
                )?
            };
        }
        resolve!(secret, redaction_secret);
        resolve!(domain, redaction_domain);
        resolve!(url, redaction_url);
        resolve!(email, redaction_email);
        resolve!(ip, redaction_ip);
        resolve!(cidr, redaction_cidr);
        resolve!(phone, redaction_phone);
        resolve!(person, redaction_person);
        resolve!(organization, redaction_organization);
        Ok(rules)
    }

    fn redaction_rule_value(
        &self,
        config_key: &str,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileRedactionRules) -> Option<bool>,
        fallback: bool,
    ) -> Result<bool, String> {
        if let Some(raw) = raw_getter(&self.env).or_else(|| raw_getter(&self.git)) {
            return parse_git_bool(raw)
                .ok_or_else(|| format!("invalid {config_key} value {:?}", raw));
        }
        Ok(self
            .file
            .as_ref()
            .and_then(|cfg| cfg.redaction_rules.as_ref())
            .and_then(file_getter)
            .unwrap_or(fallback))
    }
}
