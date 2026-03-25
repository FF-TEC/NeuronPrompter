// =============================================================================
// Input validation functions.
//
// Each function enforces a single validation rule and returns a CoreError
// with the field name and a human-readable message on failure. These are
// called by the application service layer before persisting data.
// =============================================================================

use std::net::IpAddr;

use crate::CoreError;

/// Validates a username: must be non-empty, contain only lowercase
/// alphanumeric characters and underscores, and have no spaces.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the username is empty or contains
/// characters outside the allowed set `[a-z0-9_]`.
pub fn validate_username(username: &str) -> Result<(), CoreError> {
    if username.is_empty() {
        return Err(CoreError::Validation {
            field: "username".to_owned(),
            message: "Username must not be empty".to_owned(),
        });
    }
    if username.len() > 50 {
        return Err(CoreError::Validation {
            field: "username".to_owned(),
            message: "Username must not exceed 50 characters".to_owned(),
        });
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(CoreError::Validation {
            field: "username".to_owned(),
            message: "Username must contain only lowercase alphanumeric characters and underscores"
                .to_owned(),
        });
    }
    Ok(())
}

/// Validates a prompt title: must be non-empty and at most 200 characters.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the title is empty or exceeds 200 characters.
pub fn validate_title(title: &str) -> Result<(), CoreError> {
    if title.trim().is_empty() {
        return Err(CoreError::Validation {
            field: "title".to_owned(),
            message: "Title must not be empty or whitespace-only".to_owned(),
        });
    }
    if title.chars().count() > 200 {
        return Err(CoreError::Validation {
            field: "title".to_owned(),
            message: "Title must not exceed 200 characters".to_owned(),
        });
    }
    Ok(())
}

/// Validates an optional ISO 639-1 language code: when present, must be
/// exactly 2 lowercase ASCII letters.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the language code is not exactly 2
/// lowercase letters.
pub fn validate_language_code(lang: Option<&str>) -> Result<(), CoreError> {
    if let Some(code) = lang
        && (code.len() != 2 || !code.chars().all(|c| c.is_ascii_lowercase()))
    {
        return Err(CoreError::Validation {
            field: "language".to_owned(),
            message: "Language code must be exactly 2 lowercase letters (ISO 639-1)".to_owned(),
        });
    }
    Ok(())
}

/// Validates prompt content: must be non-empty.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the content string is empty.
pub fn validate_content(content: &str) -> Result<(), CoreError> {
    if content.trim().is_empty() {
        return Err(CoreError::Validation {
            field: "content".to_owned(),
            message: "Content must not be empty or whitespace-only".to_owned(),
        });
    }
    Ok(())
}

/// Validates a chain separator: must not be empty and at most 100 characters.
///
/// Whitespace-only separators (e.g. "\n\n") are intentionally allowed because
/// newline-based separators are a common use case for joining chain steps.
/// The length check uses `chars().count()` rather than `len()` to correctly
/// handle multi-byte UTF-8 characters.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the separator is empty or exceeds 100 characters.
pub fn validate_separator(separator: &str) -> Result<(), CoreError> {
    if separator.is_empty() {
        return Err(CoreError::Validation {
            field: "separator".to_owned(),
            message: "Separator must not be empty".to_owned(),
        });
    }
    if separator.chars().count() > 100 {
        return Err(CoreError::Validation {
            field: "separator".to_owned(),
            message: "Separator must not exceed 100 characters".to_owned(),
        });
    }
    Ok(())
}

/// Validates chain steps: must contain at least one prompt ID.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the prompt list is empty.
pub fn validate_chain_steps(prompt_ids: &[i64]) -> Result<(), CoreError> {
    if prompt_ids.is_empty() {
        return Err(CoreError::Validation {
            field: "prompt_ids".to_owned(),
            message: "A chain must contain at least one prompt".to_owned(),
        });
    }
    Ok(())
}

/// Checks whether any prompt ID appears more than once in the chain steps.
/// Returns `true` if duplicates are found (used for warnings, not errors).
#[must_use]
pub fn chain_has_duplicate_prompts(prompt_ids: &[i64]) -> bool {
    let mut seen = std::collections::HashSet::new();
    prompt_ids.iter().any(|id| !seen.insert(id))
}

/// Validates a programming language identifier for scripts: must be non-empty,
/// at most 30 characters, and contain only lowercase alphanumeric characters
/// and hyphens.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the script language is empty, too long,
/// or contains invalid characters.
pub fn validate_script_language(lang: &str) -> Result<(), CoreError> {
    if lang.is_empty() {
        return Err(CoreError::Validation {
            field: "script_language".to_owned(),
            message: "Script language must not be empty".to_owned(),
        });
    }
    if lang.len() > 30 {
        return Err(CoreError::Validation {
            field: "script_language".to_owned(),
            message: "Script language must not exceed 30 characters".to_owned(),
        });
    }
    if !lang
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(CoreError::Validation {
            field: "script_language".to_owned(),
            message:
                "Script language must contain only lowercase alphanumeric characters and hyphens"
                    .to_owned(),
        });
    }
    if lang.starts_with('-') || lang.ends_with('-') {
        return Err(CoreError::Validation {
            field: "script_language".to_owned(),
            message: "Script language must not start or end with a hyphen".to_owned(),
        });
    }
    Ok(())
}

/// Validates mixed chain steps: must contain at least one step, and each
/// step must have a valid step_type.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the steps list is empty or contains
/// an invalid step_type.
pub fn validate_chain_steps_mixed(
    steps: &[crate::domain::chain::ChainStepInput],
) -> Result<(), CoreError> {
    if steps.is_empty() {
        return Err(CoreError::Validation {
            field: "steps".to_owned(),
            message: "A chain must contain at least one step".to_owned(),
        });
    }
    if steps.len() > 1000 {
        return Err(CoreError::Validation {
            field: "steps".to_owned(),
            message: "chain cannot have more than 1000 steps".to_owned(),
        });
    }
    // With the StepType enum, serde deserialization already rejects invalid
    // step types before this function is called, so no per-step validation
    // is needed here.
    Ok(())
}

/// Returns `true` if the given IP address belongs to a private (RFC 1918)
/// network range. Private network ranges are permitted for LAN deployments
/// where Ollama runs on a separate machine within the local network.
///
/// Covered ranges:
///   - 10.0.0.0/8     (10.0.0.0 -- 10.255.255.255)
///   - 172.16.0.0/12  (172.16.0.0 -- 172.31.255.255)
///   - 192.168.0.0/16 (192.168.0.0 -- 192.168.255.255)
fn is_private_ipv4(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 10.0.0.0/8
            if octets[0] == 10 {
                return true;
            }
            // 172.16.0.0/12: second octet 16..=31
            if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                return true;
            }
            // 192.168.0.0/16
            if octets[0] == 192 && octets[1] == 168 {
                return true;
            }
            false
        }
        IpAddr::V6(_) => false,
    }
}

/// Validates that a URL is safe for Ollama connections (no SSRF).
///
/// Accepts `http` and `https` schemes only. The host must be localhost,
/// a loopback address (127.0.0.1, ::1), or a private network IP address
/// (10.x.x.x, 172.16-31.x.x, 192.168.x.x). Private network ranges are
/// permitted because NeuronPrompter supports LAN deployments where Ollama
/// runs on a separate machine within the local network.
///
/// URLs with userinfo (`user:pass@host`) are rejected to prevent SSRF.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the URL is malformed, uses an
/// unsupported scheme, contains userinfo, or targets a non-allowed host.
pub fn validate_ollama_url(raw: &str) -> Result<(), CoreError> {
    let parsed = url::Url::parse(raw).map_err(|e| CoreError::Validation {
        field: "base_url".to_owned(),
        message: format!("Invalid URL: {e}"),
    })?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(CoreError::Validation {
            field: "base_url".to_owned(),
            message: format!(
                "Unsupported URL scheme '{}'; only http and https are allowed",
                parsed.scheme()
            ),
        });
    }

    // Reject userinfo to prevent SSRF via http://localhost@evil.com style URLs.
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(CoreError::Validation {
            field: "base_url".to_owned(),
            message: "URL must not contain userinfo (user:password@)".to_owned(),
        });
    }

    let host = parsed.host_str().ok_or_else(|| CoreError::Validation {
        field: "base_url".to_owned(),
        message: "URL must contain a host".to_owned(),
    })?;

    // Check if the host is "localhost" (hostname-based loopback).
    if host == "localhost" {
        return Ok(());
    }

    // Attempt to parse the host as an IP address. url::Url::host_str() returns
    // IPv6 addresses without brackets, so we try parsing directly.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if ip.is_loopback() || is_private_ipv4(&ip) {
            return Ok(());
        }
    }

    // Also accept bracketed IPv6 notation (e.g. "[::1]") for loopback.
    if host == "[::1]" {
        return Ok(());
    }

    Err(CoreError::Validation {
        field: "base_url".to_owned(),
        message: format!(
            "Ollama base URL host '{host}' is not allowed; \
             must be localhost, 127.0.0.1, [::1], or a private network IP (10.x, 172.16-31.x, 192.168.x)"
        ),
    })
}

/// Validates a description field: must not exceed 10,000 characters.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the description exceeds 10,000 characters.
pub fn validate_description(desc: &str) -> Result<(), CoreError> {
    if desc.chars().count() > 10_000 {
        return Err(CoreError::Validation {
            field: "description".to_owned(),
            message: "Description must not exceed 10,000 characters".to_owned(),
        });
    }
    Ok(())
}

/// Validates a notes field: must not exceed 50,000 characters.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the notes exceed 50,000 characters.
pub fn validate_notes(notes: &str) -> Result<(), CoreError> {
    if notes.chars().count() > 50_000 {
        return Err(CoreError::Validation {
            field: "notes".to_owned(),
            message: "Notes must not exceed 50,000 characters".to_owned(),
        });
    }
    Ok(())
}

/// Validates a display name: must not exceed 200 characters and must not
/// contain null bytes.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the display name exceeds 200 characters
/// or contains a null byte.
pub fn validate_display_name(name: &str) -> Result<(), CoreError> {
    if name.contains('\0') {
        return Err(CoreError::Validation {
            field: "display_name".to_owned(),
            message: "Display name must not contain null bytes".to_owned(),
        });
    }
    if name.trim().is_empty() {
        return Err(CoreError::Validation {
            field: "display_name".to_owned(),
            message: "Display name must not be empty or whitespace-only".to_owned(),
        });
    }
    if name.chars().count() > 200 {
        return Err(CoreError::Validation {
            field: "display_name".to_owned(),
            message: "Display name must not exceed 200 characters".to_owned(),
        });
    }
    Ok(())
}

/// Default maximum content size in bytes (1 MB).
pub const MAX_CONTENT_BYTES: usize = 1_048_576;

/// Validates that content does not exceed `max_bytes`.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the content exceeds the size limit.
pub fn validate_content_size(content: &str, max_bytes: usize) -> Result<(), CoreError> {
    if content.len() > max_bytes {
        return Err(CoreError::Validation {
            field: "content".to_owned(),
            message: format!(
                "Content exceeds maximum size of {} bytes ({} bytes provided)",
                max_bytes,
                content.len()
            ),
        });
    }
    Ok(())
}

/// Validates a taxonomy name (tag, category, or collection): must not be
/// empty or whitespace-only, and must not exceed 100 characters.
///
/// # Errors
///
/// Returns `CoreError::Validation` if the name is empty or too long.
pub fn validate_taxonomy_name(name: &str) -> Result<String, CoreError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CoreError::Validation {
            field: "name".to_owned(),
            message: "Name must not be empty or whitespace-only".to_owned(),
        });
    }
    if trimmed.chars().count() > 100 {
        return Err(CoreError::Validation {
            field: "name".to_owned(),
            message: "Name must not exceed 100 characters".to_owned(),
        });
    }
    Ok(trimmed.to_owned())
}
