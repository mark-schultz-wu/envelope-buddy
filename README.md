Initial Setup: Environment Configuration
This project uses a .env file to manage sensitive information and environment-specific settings. This file should not be committed to version control.

Create the .env file:
Copy the example environment file to a new file named .env in the root of the project:

Bash
cp .env.example .env
 Edit the .env file:
Open the newly created .env file and fill in the required values:

DISCORD_BOT_TOKEN: Your Discord Bot Token. You can get this from the Discord Developer Portal by selecting your application and going to the "Bot" page. You may need to click "Reset Token" to generate or view it.

Example: DISCORD_BOT_TOKEN="YOUR_VERY_LONG_BOT_TOKEN_HERE"
DATABASE_PATH: The path where the SQLite database file will be stored. It can be a relative path.

Example: DATABASE_PATH="data/envelope_buddy.sqlite" (Ensure the data/ directory exists or the application has rights to create it, or adjust the path as needed.)
COUPLE_USER_ID_1: The Discord User ID of the first person in the couple.

To get a User ID: In Discord, enable User Settings -> Advanced -> Developer Mode. Then, right-click on a username and select "Copy User ID".
Example: COUPLE_USER_ID_1="123456789012345678"
COUPLE_USER_ID_2: The Discord User ID of the second person in the couple.

Example: COUPLE_USER_ID_2="098765432109876543"
RUST_LOG: Controls the logging verbosity for the application.

For general information: RUST_LOG="info"
For more detailed debug logs from your bot and info from other crates: RUST_LOG="envelope_buddy=debug,info"
For very verbose tracing from your bot: RUST_LOG="envelope_buddy=trace,info"
(Optional - for development/testing slash command registration speed):

DEV_GUILD_ID: If you want to register slash commands to a specific test server (guild) for faster updates during development (instead of waiting for global propagation), uncomment this line in .env.example (if present) and provide your test server's ID.
Example: DEV_GUILD_ID="112233445566778899"
Save the .env file.

Important: The .env file is included in the .gitignore file to prevent accidental commits of your sensitive tokens and IDs. Always keep this file private.
