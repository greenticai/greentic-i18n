//! Core i18n primitives: tag canonicalization, resolver, canonical CBOR + I18nId v1.
pub mod format;
pub mod tag;
pub use format::{BasicBackend, DecimalLike, FormatBackend, FormatFacade};

use std::{
    collections::{HashMap, VecDeque},
    fmt,
    str::FromStr,
    sync::{Arc, RwLock},
};

use blake3::Hasher;
use data_encoding::BASE32_NOPAD;

use crate::tag::{
    build_parent_chain, direction_for_language, extension_value, lenient_first_day,
    lenient_hour_cycle, parse_tag_details,
};

/// Represents a canonicalized locale tag.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct I18nTag(String);

impl I18nTag {
    pub fn new(value: &str) -> Result<Self, I18nError> {
        canonicalize_tag(value).map(I18nTag)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Canonical ID derived from a resolved profile (BLAKE3 digest over canonical CBOR).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct I18nId([u8; 16]);

impl fmt::Debug for I18nId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "I18nId({})", self.as_str())
    }
}

impl fmt::Display for I18nId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl I18nId {
    pub fn zero() -> Self {
        Self::default()
    }

    pub fn version(&self) -> &'static str {
        "v1"
    }

    pub fn bytes(&self) -> [u8; 16] {
        self.0
    }

    pub fn as_str(&self) -> String {
        format!("i18n:v1:{}", BASE32_NOPAD.encode(&self.0))
    }

    pub fn from_profile(profile: &I18nProfile) -> Self {
        let canonical = profile.canonical_bytes();
        let mut hasher = Hasher::new();
        hasher.update(&canonical);
        Self::from_digest(hasher.finalize().as_bytes())
    }

    fn from_digest(digest: &[u8]) -> Self {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        Self(bytes)
    }

    pub fn parse(input: &str) -> Result<Self, I18nError> {
        let prefix = "i18n:v1:";
        if !input.starts_with(prefix) {
            return Err(I18nError::InvalidId(input.to_string()));
        }
        let encoded = &input[prefix.len()..].to_ascii_uppercase();
        let data = BASE32_NOPAD
            .decode(encoded.as_bytes())
            .map_err(I18nError::DecodeId)?;
        if data.len() < 16 {
            return Err(I18nError::InvalidId(input.to_string()));
        }
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&data[..16]);
        Ok(Self(bytes))
    }
}

impl FromStr for I18nId {
    type Err = I18nError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        I18nId::parse(s)
    }
}

/// Errors surfaced by the i18n core helpers.
#[derive(Debug)]
pub enum I18nError {
    EmptyTag,
    InvalidId(String),
    DecodeId(data_encoding::DecodeError),
    MissingField(&'static str),
}

impl fmt::Display for I18nError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            I18nError::EmptyTag => write!(f, "locale tag cannot be empty"),
            I18nError::InvalidId(value) => write!(f, "invalid I18nId `{value}`"),
            I18nError::DecodeId(err) => write!(f, "failed to decode I18nId: {err}"),
            I18nError::MissingField(field) => write!(f, "missing required field `{field}`"),
        }
    }
}

impl std::error::Error for I18nError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            I18nError::DecodeId(err) => Some(err),
            _ => None,
        }
    }
}

impl From<data_encoding::DecodeError> for I18nError {
    fn from(err: data_encoding::DecodeError) -> Self {
        I18nError::DecodeId(err)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Direction::Ltr => write!(f, "ltr"),
            Direction::Rtl => write!(f, "rtl"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I18nProfile {
    pub tag: I18nTag,
    pub currency: Option<String>,
    pub decimal_separator: char,
    pub direction: Direction,
    pub calendar: String,
    pub numbering_system: String,
    pub timezone: String,
    pub first_day: String,
    pub hour_cycle: String,
    pub collation: Option<String>,
    pub case_first: Option<String>,
    pub units: Option<String>,
    pub id: I18nId,
}

impl I18nProfile {
    #[allow(clippy::too_many_arguments)]
    fn new(
        tag: I18nTag,
        currency: Option<String>,
        direction: Direction,
        calendar: String,
        numbering_system: String,
        timezone: String,
        first_day: String,
        hour_cycle: String,
        collation: Option<String>,
        case_first: Option<String>,
        units: Option<String>,
    ) -> Self {
        let decimal_separator = decimal_separator_for_tag(&tag);
        let mut profile = I18nProfile {
            tag,
            currency,
            decimal_separator,
            direction,
            calendar,
            numbering_system,
            timezone,
            first_day,
            hour_cycle,
            collation,
            case_first,
            units,
            id: I18nId::zero(),
        };
        profile.id = I18nId::from_profile(&profile);
        profile
    }

    fn canonical_bytes(&self) -> Vec<u8> {
        encode_canonical_profile(self)
    }
}

#[derive(Debug, Clone)]
pub struct I18nRequest {
    pub user_tag: Option<I18nTag>,
    pub session_tag: Option<I18nTag>,
    pub content_tag: Option<I18nTag>,
    pub currency: Option<String>,
    pub timezone: Option<String>,
    pub mode: ResolveMode,
}

impl I18nRequest {
    pub fn new(tag: Option<I18nTag>, currency: Option<String>) -> Self {
        Self {
            user_tag: None,
            session_tag: None,
            content_tag: tag,
            currency,
            timezone: None,
            mode: ResolveMode::Lenient,
        }
    }

    pub fn with_mode(mut self, mode: ResolveMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_timezone(mut self, tz: impl Into<String>) -> Self {
        self.timezone = Some(tz.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResolveMode {
    Strict,
    #[default]
    Lenient,
}

#[derive(Debug, Clone)]
pub struct I18nResolution {
    pub id: I18nId,
    pub profile: I18nProfile,
    pub fallback_chain: Vec<I18nTag>,
}

pub trait I18nResolver: Send + Sync + 'static {
    fn resolve(&self, req: I18nRequest) -> Result<I18nResolution, I18nError>;
}

pub struct DefaultResolver {
    tenant_default: I18nTag,
    default_currency: Option<String>,
}

impl Default for DefaultResolver {
    fn default() -> Self {
        Self {
            tenant_default: I18nTag::new("en-US").expect("valid default tag"),
            default_currency: Some("USD".to_string()),
        }
    }
}

impl DefaultResolver {
    pub fn new(tenant_default: I18nTag, default_currency: Option<String>) -> Self {
        Self {
            tenant_default,
            default_currency,
        }
    }
}

impl I18nResolver for DefaultResolver {
    fn resolve(&self, req: I18nRequest) -> Result<I18nResolution, I18nError> {
        let mut currency = req
            .currency
            .clone()
            .or_else(|| self.default_currency.clone());
        let chosen_tag = req
            .content_tag
            .clone()
            .or(req.session_tag.clone())
            .or(req.user_tag.clone())
            .unwrap_or_else(|| self.tenant_default.clone());
        let fallback_chain = build_fallback_chain(&chosen_tag, &self.tenant_default);
        let details = parse_tag_details(&chosen_tag);
        let direction = direction_for_language(&details.language);

        let calendar = extension_value(&details, "ca")
            .or_else(|| Some(lenient_calendar()))
            .unwrap();
        let numbering_system = extension_value(&details, "nu")
            .or_else(|| Some(lenient_numbering_system()))
            .unwrap();
        let timezone = extension_value(&details, "tz")
            .or_else(|| req.timezone.clone())
            .unwrap_or_else(|| "UTC".to_string());

        if req.mode == ResolveMode::Strict
            && extension_value(&details, "tz").is_none()
            && req.timezone.is_none()
        {
            return Err(I18nError::MissingField("timezone"));
        }
        if req.mode == ResolveMode::Strict && extension_value(&details, "ca").is_none() {
            return Err(I18nError::MissingField("calendar"));
        }
        if req.mode == ResolveMode::Strict && extension_value(&details, "nu").is_none() {
            return Err(I18nError::MissingField("numbering_system"));
        }

        let first_day = lenient_first_day(details.region.as_deref()).to_string();
        let hour_cycle = lenient_hour_cycle(details.region.as_deref()).to_string();
        let collation = extension_value(&details, "co");
        let case_first = extension_value(&details, "kf");
        let units = extension_value(&details, "unit");

        let profile = I18nProfile::new(
            chosen_tag.clone(),
            currency.take(),
            direction,
            calendar,
            numbering_system,
            timezone,
            first_day,
            hour_cycle,
            collation,
            case_first,
            units,
        );

        let resolution = I18nResolution {
            id: profile.id,
            profile,
            fallback_chain,
        };
        Ok(resolution)
    }
}

pub struct I18n {
    resolver: Arc<dyn I18nResolver>,
    cache: RwLock<I18nCache>,
}

impl I18n {
    pub fn new(resolver: Arc<dyn I18nResolver>) -> Self {
        Self::new_with_config(resolver, I18nCacheConfig::default())
    }

    pub fn new_with_config(resolver: Arc<dyn I18nResolver>, config: I18nCacheConfig) -> Self {
        Self {
            resolver,
            cache: RwLock::new(I18nCache::new(config)),
        }
    }

    pub fn profile(&self, id: &I18nId) -> Option<I18nProfile> {
        self.get(id).map(|profile| (*profile).clone())
    }

    pub fn get(&self, id: &I18nId) -> Option<Arc<I18nProfile>> {
        self.cache.read().unwrap().get(id)
    }

    pub fn get_with_fallback(&self, id: &I18nId) -> Option<I18nCacheSnapshot> {
        self.cache.read().unwrap().get_snapshot(id)
    }

    pub fn insert(&self, profile: I18nProfile, fallback_chain: Vec<I18nTag>) -> I18nId {
        let mut stored = profile.clone();
        let id = I18nId::from_profile(&stored);
        stored.id = id;
        let entry = I18nCacheEntry {
            profile: Arc::new(stored),
            fallback_chain,
        };
        self.cache.write().unwrap().insert(id, entry);
        id
    }

    pub fn resolve_and_cache(&self, req: I18nRequest) -> Result<I18nResolution, I18nError> {
        let resolution = self.resolver.resolve(req)?;
        let entry = I18nCacheEntry {
            profile: Arc::new(resolution.profile.clone()),
            fallback_chain: resolution.fallback_chain.clone(),
        };
        self.cache.write().unwrap().insert(resolution.id, entry);
        Ok(resolution)
    }
}

fn decimal_separator_for_tag(tag: &I18nTag) -> char {
    if tag.as_str().starts_with("fr-") {
        ','
    } else {
        '.'
    }
}

fn lenient_calendar() -> String {
    "gregory".to_string()
}

fn lenient_numbering_system() -> String {
    "latn".to_string()
}

fn build_fallback_chain(final_tag: &I18nTag, tenant_default: &I18nTag) -> Vec<I18nTag> {
    let mut chain = Vec::new();
    let parents = build_parent_chain(final_tag);
    for tag in parents {
        if chain
            .iter()
            .any(|existing: &I18nTag| existing.as_str() == tag.as_str())
        {
            continue;
        }
        chain.push(tag);
    }
    if !chain.iter().any(|t| t == tenant_default) {
        chain.push(tenant_default.clone());
    }
    chain
}

#[derive(Clone)]
pub struct I18nCacheConfig {
    pub max_entries: usize,
}

impl Default for I18nCacheConfig {
    fn default() -> Self {
        Self { max_entries: 1024 }
    }
}

pub struct I18nCacheEntry {
    profile: Arc<I18nProfile>,
    fallback_chain: Vec<I18nTag>,
}

pub struct I18nCacheSnapshot {
    pub profile: Arc<I18nProfile>,
    pub fallback_chain: Vec<I18nTag>,
}

pub struct I18nCache {
    entries: HashMap<I18nId, I18nCacheEntry>,
    order: VecDeque<(I18nId, u64)>,
    recent: HashMap<I18nId, u64>,
    next_touch: u64,
    config: I18nCacheConfig,
}

impl I18nCache {
    fn new(config: I18nCacheConfig) -> Self {
        let max_entries = if config.max_entries == 0 {
            1
        } else {
            config.max_entries
        };
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            recent: HashMap::new(),
            next_touch: 0,
            config: I18nCacheConfig { max_entries },
        }
    }

    fn insert(&mut self, id: I18nId, entry: I18nCacheEntry) {
        self.entries.insert(id, entry);
        self.touch(&id);
        self.evict_if_needed();
    }

    fn get(&self, id: &I18nId) -> Option<Arc<I18nProfile>> {
        self.entries.get(id).map(|entry| entry.profile.clone())
    }

    fn get_snapshot(&self, id: &I18nId) -> Option<I18nCacheSnapshot> {
        let entry = self.entries.get(id)?;
        Some(I18nCacheSnapshot {
            profile: entry.profile.clone(),
            fallback_chain: entry.fallback_chain.clone(),
        })
    }

    fn touch(&mut self, id: &I18nId) {
        self.next_touch = self.next_touch.wrapping_add(1);
        self.recent.insert(*id, self.next_touch);
        self.order.push_back((*id, self.next_touch));
    }

    fn evict_if_needed(&mut self) {
        while self.entries.len() > self.config.max_entries {
            if let Some((evicted, seen_at)) = self.order.pop_front() {
                let is_current = self
                    .recent
                    .get(&evicted)
                    .is_some_and(|current| *current == seen_at);
                if is_current {
                    self.entries.remove(&evicted);
                    self.recent.remove(&evicted);
                }
            }
        }
    }
}

fn encode_canonical_profile(profile: &I18nProfile) -> Vec<u8> {
    let mut entries: Vec<(&str, String)> = vec![
        ("calendar", profile.calendar.clone()),
        ("decimal_separator", profile.decimal_separator.to_string()),
        ("direction", profile.direction.to_string()),
        ("first_day", profile.first_day.clone()),
        ("hour_cycle", profile.hour_cycle.clone()),
        ("numbering_system", profile.numbering_system.clone()),
        ("tag", profile.tag.as_str().to_string()),
        ("timezone", profile.timezone.clone()),
    ];

    if let Some(currency) = &profile.currency {
        entries.push(("currency", currency.clone()));
    }
    if let Some(collation) = &profile.collation {
        entries.push(("collation", collation.clone()));
    }
    if let Some(case_first) = &profile.case_first {
        entries.push(("case_first", case_first.clone()));
    }
    if let Some(units) = &profile.units {
        entries.push(("units", units.clone()));
    }

    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut buf = Vec::new();
    encode_map_header(entries.len(), &mut buf);
    for (key, value) in entries {
        encode_text(key, &mut buf);
        encode_text(&value, &mut buf);
    }
    buf
}

fn encode_map_header(len: usize, buf: &mut Vec<u8>) {
    encode_unsigned(5, len as u64, buf);
}

fn encode_text(value: &str, buf: &mut Vec<u8>) {
    let bytes = value.as_bytes();
    encode_unsigned(3, bytes.len() as u64, buf);
    buf.extend_from_slice(bytes);
}

fn encode_unsigned(major: u8, value: u64, buf: &mut Vec<u8>) {
    if value < 24 {
        buf.push((major << 5) | (value as u8));
    } else if value < 256 {
        buf.push((major << 5) | 24);
        buf.push(value as u8);
    } else if value < 65_536 {
        buf.push((major << 5) | 25);
        buf.extend_from_slice(&(value as u16).to_be_bytes());
    } else if value < 4_294_967_296 {
        buf.push((major << 5) | 26);
        buf.extend_from_slice(&(value as u32).to_be_bytes());
    } else {
        buf.push((major << 5) | 27);
        buf.extend_from_slice(&value.to_be_bytes());
    }
}

pub fn normalize_tag(input: &str) -> Result<I18nTag, I18nError> {
    canonicalize_tag(input).map(I18nTag)
}

fn canonicalize_tag(input: &str) -> Result<String, I18nError> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(I18nError::EmptyTag);
    }

    let mut canonical: Vec<String> = Vec::new();
    let mut in_extension = false;
    for part in raw.split('-').filter(|p| !p.is_empty()) {
        if !in_extension && part.eq_ignore_ascii_case("u") {
            in_extension = true;
            canonical.push("u".to_string());
            continue;
        }

        let normalized = if in_extension || canonical.is_empty() {
            part.to_ascii_lowercase()
        } else if part.len() == 4 {
            let mut chars = part.chars();
            let first = chars.next().unwrap().to_ascii_uppercase();
            let rest: String = chars.map(|c| c.to_ascii_lowercase()).collect();
            format!("{first}{rest}")
        } else if part.len() <= 3 {
            part.to_ascii_uppercase()
        } else {
            part.to_ascii_lowercase()
        };

        canonical.push(normalized);
    }

    if canonical.is_empty() {
        Err(I18nError::EmptyTag)
    } else {
        Ok(canonical.join("-"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DefaultResolver, Direction, I18nCacheConfig, I18nError, I18nId, I18nProfile, I18nRequest,
        ResolveMode, normalize_tag,
    };
    use crate::{I18n, I18nResolver};
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn normalize_common_tags() {
        assert_eq!(normalize_tag("en-gb").unwrap().as_str(), "en-GB");
        assert_eq!(normalize_tag("zh-hant-tw").unwrap().as_str(), "zh-Hant-TW");
        assert_eq!(
            normalize_tag("EN-us-U-ca-gregory-cu-usd").unwrap().as_str(),
            "en-US-u-ca-gregory-cu-usd"
        );
    }

    #[test]
    fn canonical_profile_id_is_stable() {
        let tag = normalize_tag("en-GB-u-ca-gregory-cu-gbp").unwrap();
        let profile = I18nProfile::new(
            tag,
            Some("GBP".to_string()),
            super::Direction::Ltr,
            "gregory".to_string(),
            "latn".to_string(),
            "UTC".to_string(),
            "mon".to_string(),
            "h23".to_string(),
            None,
            None,
            None,
        );
        assert_eq!(profile.id.as_str(), "i18n:v1:KU23J7EOLPRYIEJRBTXNCBMQBA");
    }

    #[test]
    fn fixture_matches_expected_canonicalization() {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("i18n_id_v1_cases.json");
        let raw = fs::read_to_string(fixture_path).expect("fixture file");
        let cases: serde_json::Value =
            serde_json::from_str(&raw).expect("fixture JSON should be valid");
        for case in cases.as_array().unwrap() {
            let tag = case["tag"].as_str().unwrap();
            let currency = case["currency"].as_str();
            let normalized = normalize_tag(tag).unwrap();
            let profile = I18nProfile::new(
                normalized.clone(),
                currency.map(|c| c.to_string()),
                super::Direction::Ltr,
                "gregory".to_string(),
                "latn".to_string(),
                "UTC".to_string(),
                "mon".to_string(),
                "h23".to_string(),
                None,
                None,
                None,
            );
            let hex = profile
                .canonical_bytes()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            assert_eq!(hex, case["cbor_hex"].as_str().unwrap(), "{}", tag);
            assert_eq!(
                profile.id.as_str(),
                case["expected_id"].as_str().unwrap(),
                "{}",
                tag
            );
        }
    }

    #[test]
    fn resolver_precedence_prefers_content_tag() {
        let tenant = normalize_tag("en-US").unwrap();
        let resolver = DefaultResolver::new(tenant.clone(), Some("USD".to_string()));
        let request = I18nRequest {
            user_tag: Some(normalize_tag("fr-FR").unwrap()),
            session_tag: Some(normalize_tag("de-DE").unwrap()),
            content_tag: Some(normalize_tag("ar-OM").unwrap()),
            currency: None,
            timezone: None,
            mode: ResolveMode::Lenient,
        };
        let resolution = resolver.resolve(request).unwrap();
        assert_eq!(resolution.profile.tag.as_str(), "ar-OM");
        assert_eq!(resolution.fallback_chain.first().unwrap().as_str(), "ar-OM");
        assert_eq!(resolution.fallback_chain.last().unwrap(), &tenant);
    }

    #[test]
    fn lenient_defaults_follow_region_rules() {
        let resolver = DefaultResolver::default();
        let request = I18nRequest {
            user_tag: None,
            session_tag: None,
            content_tag: Some(normalize_tag("en-US").unwrap()),
            currency: None,
            timezone: None,
            mode: ResolveMode::Lenient,
        };
        let resolution = resolver.resolve(request).unwrap();
        assert_eq!(resolution.profile.first_day, "sun");
        assert_eq!(resolution.profile.hour_cycle, "h12");
        assert_eq!(resolution.profile.direction, super::Direction::Ltr);
    }

    #[test]
    fn strict_mode_requires_timezone() {
        let resolver = DefaultResolver::default();
        let request = I18nRequest {
            user_tag: None,
            session_tag: None,
            content_tag: Some(normalize_tag("en-US").unwrap()),
            currency: None,
            timezone: None,
            mode: ResolveMode::Strict,
        };
        let err = resolver.resolve(request).unwrap_err();
        assert!(matches!(err, I18nError::MissingField("timezone")));
    }

    #[test]
    fn strict_mode_requires_calendar_and_numbering() {
        let resolver = DefaultResolver::default();
        let mut request = I18nRequest {
            user_tag: None,
            session_tag: None,
            content_tag: Some(normalize_tag("en-US").unwrap()),
            currency: None,
            timezone: Some("UTC".to_string()),
            mode: ResolveMode::Strict,
        };
        let err = resolver.resolve(request.clone()).unwrap_err();
        assert!(matches!(err, I18nError::MissingField("calendar")));

        request.content_tag = Some(normalize_tag("fr-FR-u-ca-gregory").unwrap());
        let err = resolver.resolve(request).unwrap_err();
        assert!(matches!(err, I18nError::MissingField("numbering_system")));
    }

    #[test]
    fn resolver_precedence_table_is_deterministic() {
        let tenant_default = normalize_tag("en-US").unwrap();
        let resolver = DefaultResolver::new(tenant_default.clone(), Some("USD".to_string()));
        let cases = [
            (None, None, None, "en-US"),
            (Some("fr-FR"), None, None, "fr-FR"),
            (None, Some("de-DE"), None, "de-DE"),
            (Some("fr-FR"), Some("de-DE"), None, "de-DE"),
            (Some("fr-FR"), Some("de-DE"), Some("es-ES"), "es-ES"),
        ];

        for (user, session, content, expected) in cases {
            let request = I18nRequest {
                user_tag: user.map(|tag| normalize_tag(tag).unwrap()),
                session_tag: session.map(|tag| normalize_tag(tag).unwrap()),
                content_tag: content.map(|tag| normalize_tag(tag).unwrap()),
                currency: None,
                timezone: Some("UTC".to_string()),
                mode: ResolveMode::Lenient,
            };
            let resolution = resolver.resolve(request).unwrap();
            assert_eq!(resolution.profile.tag.as_str(), expected);
            assert_eq!(
                resolution.fallback_chain.first().unwrap().as_str(),
                expected
            );
        }
    }

    #[test]
    fn lenient_mode_derives_deterministic_defaults() {
        let resolver = DefaultResolver::default();
        let request = I18nRequest {
            user_tag: None,
            session_tag: None,
            content_tag: Some(normalize_tag("ar-SA").unwrap()),
            currency: None,
            timezone: Some("Asia/Riyadh".to_string()),
            mode: ResolveMode::Lenient,
        };
        let resolution = resolver.resolve(request).unwrap();
        assert_eq!(resolution.profile.direction, Direction::Rtl);
        assert_eq!(resolution.profile.first_day, "sat");
        assert_eq!(resolution.profile.hour_cycle, "h23");
        assert_eq!(resolution.profile.calendar, "gregory");
        assert_eq!(resolution.profile.numbering_system, "latn");
        assert_eq!(resolution.profile.timezone, "Asia/Riyadh");
        assert_eq!(resolution.profile.tag.as_str(), "ar-SA");

        let fallback_request = I18nRequest {
            user_tag: None,
            session_tag: None,
            content_tag: Some(normalize_tag("en-US").unwrap()),
            currency: None,
            timezone: None,
            mode: ResolveMode::Lenient,
        };
        let fallback_resolution = resolver.resolve(fallback_request).unwrap();
        assert_eq!(fallback_resolution.profile.timezone, "UTC");
    }

    #[test]
    fn fallback_chain_reuses_tenant_parent() {
        let tenant_default = normalize_tag("en").unwrap();
        let resolver = DefaultResolver::new(tenant_default.clone(), None);
        let request = I18nRequest {
            user_tag: None,
            session_tag: None,
            content_tag: Some(normalize_tag("en-US").unwrap()),
            currency: None,
            timezone: None,
            mode: ResolveMode::Lenient,
        };
        let resolution = resolver.resolve(request).unwrap();
        let chain: Vec<_> = resolution
            .fallback_chain
            .iter()
            .map(|tag| tag.as_str().to_string())
            .collect();
        assert_eq!(chain, vec!["en-US".to_string(), "en".to_string()]);
    }

    #[test]
    fn cached_profile_matches_id_and_fallback_chain() {
        let resolver = DefaultResolver::default();
        let engine = I18n::new(Arc::new(resolver));
        let resolution = engine
            .resolve_and_cache(I18nRequest::new(
                Some(normalize_tag("en-US").unwrap()),
                None,
            ))
            .unwrap();
        let cached = engine
            .get_with_fallback(&resolution.id)
            .expect("missing cache entry");
        assert_eq!(cached.profile.id, resolution.id);
        assert_eq!(cached.fallback_chain, resolution.fallback_chain);
        assert_eq!(I18nId::from_profile(&cached.profile), resolution.id);
    }

    #[test]
    fn cache_respects_max_entries_limit() {
        let resolver = DefaultResolver::default();
        let engine = I18n::new_with_config(Arc::new(resolver), I18nCacheConfig { max_entries: 2 });
        let first = engine
            .resolve_and_cache(I18nRequest::new(
                Some(normalize_tag("en-US").unwrap()),
                None,
            ))
            .unwrap()
            .id;
        let second = engine
            .resolve_and_cache(I18nRequest::new(
                Some(normalize_tag("fr-FR").unwrap()),
                None,
            ))
            .unwrap()
            .id;
        let third = engine
            .resolve_and_cache(I18nRequest::new(
                Some(normalize_tag("ar-OM").unwrap()),
                None,
            ))
            .unwrap()
            .id;
        assert!(engine.get(&first).is_none());
        assert!(engine.get(&second).is_some());
        assert!(engine.get(&third).is_some());
    }

    #[test]
    fn cache_retains_most_recent_inserts() {
        let resolver = DefaultResolver::default();
        let engine = I18n::new_with_config(Arc::new(resolver), I18nCacheConfig { max_entries: 2 });
        let first = engine
            .resolve_and_cache(I18nRequest::new(
                Some(normalize_tag("en-US").unwrap()),
                None,
            ))
            .unwrap()
            .id;
        let second = engine
            .resolve_and_cache(I18nRequest::new(
                Some(normalize_tag("fr-FR").unwrap()),
                None,
            ))
            .unwrap()
            .id;
        let third = engine
            .resolve_and_cache(I18nRequest::new(
                Some(normalize_tag("ar-OM").unwrap()),
                None,
            ))
            .unwrap()
            .id;

        assert!(
            engine.get(&first).is_none(),
            "oldest entry should be evicted"
        );
        assert!(
            engine.get(&second).is_some(),
            "second entry should be retained"
        );
        assert!(engine.get(&third).is_some(), "new entry should be retained");
    }

    #[test]
    fn fallback_chain_includes_parents() {
        let tenant = normalize_tag("en-US").unwrap();
        let resolver = DefaultResolver::new(tenant.clone(), None);
        let request = I18nRequest {
            user_tag: None,
            session_tag: None,
            content_tag: Some(normalize_tag("en-GB").unwrap()),
            currency: None,
            timezone: None,
            mode: ResolveMode::Lenient,
        };
        let resolution = resolver.resolve(request).unwrap();
        assert!(
            resolution
                .fallback_chain
                .iter()
                .any(|tag| tag.as_str() == "en-GB")
        );
        assert!(
            resolution
                .fallback_chain
                .iter()
                .any(|tag| tag.as_str() == "en")
        );
        assert_eq!(resolution.fallback_chain.last().unwrap(), &tenant);
    }
}
