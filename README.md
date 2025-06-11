# tuimail

A terminal-based email client for Gmail.

**Note:** This project is currently under active (sort of?) development and is not yet production-ready. While bug reports are welcome, due to the early stage of development and the maintainer's evolving Rust skills, it is highly recommended that any bug reports be accompanied by a pull request with a proposed fix. Your contributions are greatly appreciated!

## Setup

### Obtaining `client_secret.json`

To use this application, you need to obtain a `client_secret.json` file from the Google Cloud Console. Follow these steps:

1.  **Go to Google Cloud Console**: Navigate to [https://console.cloud.google.com/](https://console.cloud.google.com/).
2.  **Create a New Project**:
    *   Click on the project dropdown in the header (usually next to "Google Cloud").
    *   Click "New Project".
    *   Give your project a name (e.g., "rmail-client") and click "Create".
3.  **Enable Gmail API**:
    *   Once your project is created, go to the "APIs & Services" > "Library" section.
    *   Search for "Gmail API" and select it.
    *   Click "Enable".
4.  **Configure OAuth Consent Screen**:
    *   Go to "APIs & Services" > "OAuth consent screen".
    *   Choose "External" for User Type and click "Create".
    *   Fill in the required fields (App name, User support email, Developer contact information). You can use "rmail" for the app name.
    *   For "Scopes", click "Add or Remove Scopes" and add `https://mail.google.com/` (Gmail API).
    *   Save and continue through the remaining steps. For testing, you can add your Google account as a "Test user".
5.  **Create Credentials (OAuth Client ID)**:
    *   Go to "APIs & Services" > "Credentials".
    *   Click "Create Credentials" > "OAuth client ID".
    *   Select "Desktop app" as the Application type.
    *   Give it a name (e.g., "rmail-desktop-client") and click "Create".
    *   A dialog will appear with your client ID and client secret. Click "Download JSON" to download your `client_secret.json` file.
6.  **Place `client_secret.json`**:
    *   Place the downloaded `client_secret.json` file in the root directory of this project (where `Cargo.toml` is located).
    *   The application will attempt to load this file. Once successfully loaded, it will be stored securely in your system's keyring, and you will be prompted to delete the `client_secret.json` file for security reasons.

## Running the Application

To run the application, ensure you have Rust and Cargo installed. Then, navigate to the project's root directory in your terminal and execute:

```bash
cargo run --release
```

The `--release` flag compiles the application with optimizations, resulting in better performance.

## Troubleshooting

### macOS Keychain Behavior

**Important for macOS Users**: Each time you compile or run a new build of the application (e.g., with `cargo run` or `cargo build`), macOS will treat it as a different application due to the changed binary signature. This means you will be re-prompted for your keychain password and will need to select "Always Allow" to grant the application access to stored credentials.

This is expected macOS behavior and not a bug. To minimize these prompts during development, you can use the same compiled binary multiple times rather than recompiling frequently.

### Clearing Keyring Credentials

If you encounter issues with authentication, such as `invalid_grant` errors or problems with token refresh, it might be necessary to clear the stored credentials from your system's keyring. This can happen if your authentication token expires or becomes invalid.

To clear the stored client secret and token, run the application with the `--clear-keyring` flag:

```bash
cargo run --release -- --clear-keyring
```

This command will delete the `client_secret.json` and token entries from your keyring. After running this, you will need to re-authenticate the next time you run `tuimail` normally, which will prompt you to go through the OAuth flow again. This effectively resets your authentication state.

## Contributing

This project was initially developed by someone with limited prior experience in Rust. As such, the codebase may not adhere to all Rust best practices. Contributions are highly encouraged and greatly appreciated!

We welcome any improvements, including:

* **Rust Best Practices**: Refactoring code to align with idiomatic Rust patterns and conventions.
* **Performance Enhancements**: Optimizations to improve the application's speed and efficiency.
* **Feature Additions**: New functionalities to enhance the user experience.
* **Bug Fixes**: Addressing any issues or unexpected behavior.
* **Documentation**: Improving existing documentation or adding new explanations.

Feel free to open issues or pull requests. Your contributions will help make `tuimail` a more robust and user-friendly application.

Please note that we treat compiler warnings like errors here.  If your PR is emitting warnings, it will not be merged (unless it is an emergency security patch).

## Code of Conduct

We are committed to providing a welcoming and inclusive environment for all contributors. Everyone is welcome to participate in this project regardless of their background, experience level, or identity.

### Our Values

* **All Contributors Welcome**: Whether you're a seasoned Rust developer or just getting started, your contributions are valued and appreciated.
* **Assume Good Intentions**: We approach all interactions with the assumption that contributors have good intentions and are acting in good faith.
* **Kindness First**: We prioritize kindness and respect in all communications. Disagreements should focus on ideas and code, not personal attacks.
* **Learning Together**: We recognize that everyone is learning and growing (including our lead developer). Mistakes are opportunities for improvement, not reasons for criticism.
* **Constructive Feedback**: When providing feedback, we aim to be helpful and constructive, offering suggestions for improvement rather than just pointing out problems.

### Expected Behavior

* Be respectful and considerate in all interactions
* Provide helpful and constructive feedback
* Be patient with contributors who are learning
* Assume positive intent when interpreting communications
* Focus on what is best for the community and the project

We believe that by fostering a kind and welcoming community, we can create better software together. Thank you for helping to make this project a positive experience for everyone.
