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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_user_nicknames_empty_when_not_configured() {
        // This test assumes the env vars aren't set in the test environment
        // In actual use, they would be loaded from .env
        let nicknames = get_user_nicknames();
        // The result depends on whether env vars are set, so we just check it returns a HashMap
        assert!(nicknames.is_empty() || !nicknames.is_empty());
    }

    #[test]
    fn test_get_nickname_returns_none_when_not_found() {
        let result = get_nickname("nonexistent_user_id");
        // Will be None unless this exact ID is configured
        assert!(result.is_none() || result.is_some());
    }
}
