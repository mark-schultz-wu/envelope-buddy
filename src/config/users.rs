//! User configuration module for loading user nicknames from environment variables.
//!
//! This module provides functionality to load and map Discord user IDs to friendly
//! nicknames configured in the `.env` file. Nicknames are optional and will fall back
//! to Discord usernames if not configured.

use std::collections::HashMap;

/// Gets a mapping of user IDs to their configured nicknames from environment variables.
///
/// Reads `COUPLE_USER_ID_1`, `COUPLE_USER_ID_2`, `USER_NICKNAME_1`, and `USER_NICKNAME_2`
/// from the environment and creates a `HashMap` for quick lookups.
///
/// # Returns
///
/// A `HashMap` mapping user ID strings to nickname strings. Only includes entries
/// where both the user ID and nickname are configured.
#[must_use]
pub fn get_user_nicknames() -> HashMap<String, String> {
    let mut nicknames = HashMap::new();

    // Load user 1 mapping
    if let (Ok(user_id_1), Ok(nickname_1)) = (
        std::env::var("COUPLE_USER_ID_1"),
        std::env::var("USER_NICKNAME_1"),
    ) {
        nicknames.insert(user_id_1, nickname_1);
    }

    // Load user 2 mapping
    if let (Ok(user_id_2), Ok(nickname_2)) = (
        std::env::var("COUPLE_USER_ID_2"),
        std::env::var("USER_NICKNAME_2"),
    ) {
        nicknames.insert(user_id_2, nickname_2);
    }

    nicknames
}

/// Gets the nickname for a given user ID, if configured.
///
/// # Arguments
///
/// * `user_id` - The Discord user ID to look up
///
/// # Returns
///
/// `Some(nickname)` if a nickname is configured for this user ID, `None` otherwise.
#[must_use]
pub fn get_nickname(user_id: &str) -> Option<String> {
    let nicknames = get_user_nicknames();
    nicknames.get(user_id).cloned()
}

/// Gets the user ID for a given nickname, if configured.
///
/// # Arguments
///
/// * `nickname` - The nickname to look up
///
/// # Returns
///
/// `Some(user_id)` if a user ID is configured for this nickname, `None` otherwise.
#[must_use]
pub fn get_user_id_by_nickname(nickname: &str) -> Option<String> {
    let nicknames = get_user_nicknames();
    nicknames
        .iter()
        .find(|(_, nick)| nick.to_lowercase() == nickname.to_lowercase())
        .map(|(user_id, _)| user_id.clone())
}

/// Gets all available user nicknames for autocomplete suggestions.
///
/// # Returns
///
/// A vector of all configured nicknames.
#[must_use]
pub fn get_all_nicknames() -> Vec<String> {
    get_user_nicknames().into_values().collect()
}

/// Resolves a nickname to a Discord user ID.
///
/// This function only accepts configured nicknames for simplicity.
///
/// # Arguments
///
/// * `nickname` - The nickname to resolve
///
/// # Returns
///
/// `Some(user_id)` if the nickname is configured, `None` otherwise.
#[must_use]
pub fn resolve_nickname(nickname: &str) -> Option<String> {
    get_user_id_by_nickname(nickname.trim())
}

/// Gets a display name for a user ID.
///
/// This is the reverse of `resolve_user_input` - it converts a user ID
/// back to a human-readable format for display purposes.
///
/// # Arguments
///
/// * `user_id` - The Discord user ID
///
/// # Returns
///
/// A display string: nickname if configured, otherwise the raw user ID.
#[must_use]
pub fn get_user_display_name(user_id: &str) -> String {
    get_nickname(user_id).unwrap_or_else(|| user_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use temp_env;

    #[test]
    fn test_with_temp_env() {
        temp_env::with_vars(
            vec![
                ("COUPLE_USER_ID_1", Some("123456789")),
                ("USER_NICKNAME_1", Some("Alice")),
                ("COUPLE_USER_ID_2", Some("987654321")),
                ("USER_NICKNAME_2", Some("Bob")),
            ],
            || {
                // Test get_nickname
                assert_eq!(get_nickname("123456789"), Some("Alice".to_string()));
                assert_eq!(get_nickname("987654321"), Some("Bob".to_string()));
                assert_eq!(get_nickname("nonexistent"), None);

                // Test get_user_id_by_nickname
                assert_eq!(get_user_id_by_nickname("Alice"), Some("123456789".to_string()));
                assert_eq!(get_user_id_by_nickname("Bob"), Some("987654321".to_string()));
                assert_eq!(get_user_id_by_nickname("Charlie"), None);

                // Test case insensitive
                assert_eq!(get_user_id_by_nickname("alice"), Some("123456789".to_string()));
                assert_eq!(get_user_id_by_nickname("ALICE"), Some("123456789".to_string()));

                // Test resolve_nickname (wrapper function)
                assert_eq!(resolve_nickname("Alice"), Some("123456789".to_string()));
                assert_eq!(resolve_nickname("alice"), Some("123456789".to_string()));
                assert_eq!(resolve_nickname("  Alice  "), Some("123456789".to_string())); // Trims whitespace
                assert_eq!(resolve_nickname("Charlie"), None);

                // Test get_all_nicknames
                let mut nicknames = get_all_nicknames();
                nicknames.sort();
                assert_eq!(nicknames, vec!["Alice", "Bob"]);

                // Test get_user_display_name
                assert_eq!(get_user_display_name("123456789"), "Alice");
                assert_eq!(get_user_display_name("987654321"), "Bob");
                assert_eq!(get_user_display_name("nonexistent"), "nonexistent");
            },
        );
    }

    #[test]
    fn test_partial_configuration() {
        temp_env::with_vars(
            vec![
                ("COUPLE_USER_ID_1", Some("123456789")),
                ("USER_NICKNAME_1", Some("Alice")),
                // Clear other user env vars to test partial config
                ("COUPLE_USER_ID_2", None),
                ("USER_NICKNAME_2", None),
            ],
            || {
                assert_eq!(get_nickname("123456789"), Some("Alice".to_string()));
                assert_eq!(get_nickname("987654321"), None);
                assert_eq!(get_user_id_by_nickname("Alice"), Some("123456789".to_string()));
                assert_eq!(get_user_id_by_nickname("Bob"), None);

                let nicknames = get_all_nicknames();
                assert_eq!(nicknames, vec!["Alice"]);
            },
        );
    }

    #[test]
    fn test_resolve_nickname_edge_cases() {
        temp_env::with_vars(
            vec![
                ("COUPLE_USER_ID_1", Some("123456789")),
                ("USER_NICKNAME_1", Some("Alice")),
            ],
            || {
                // Test with empty string
                assert_eq!(resolve_nickname(""), None);

                // Test with whitespace only
                assert_eq!(resolve_nickname("   "), None);

                // Test with whitespace around nickname (should be trimmed)
                assert_eq!(resolve_nickname("  Alice  "), Some("123456789".to_string()));
            },
        );
    }

    #[test]
    fn test_get_user_display_name_fallback() {
        // Test without any env vars set
        assert_eq!(get_user_display_name("123456789"), "123456789"); // Falls back to user ID
        assert_eq!(get_user_display_name("nonexistent"), "nonexistent");

        // Test with empty user ID
        assert_eq!(get_user_display_name(""), "");
    }
}
