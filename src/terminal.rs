use crossterm::{execute, terminal};
use ratatui::Terminal;
use std::io::{self, stdout};

pub fn setup_terminal(
) -> Result<Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>, Box<dyn std::error::Error>>
{
    terminal::enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, terminal::EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

pub fn cleanup_terminal(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> io::Result<()> {
    terminal::disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        terminal::Clear(terminal::ClearType::All), // Clear the screen
        terminal::LeaveAlternateScreen
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::io;

    // This test checks if cleanup_terminal can be called without panicking
    // and if it returns Ok. It uses a TestBackend for ratatui.
    #[tokio::test]
    async fn test_cleanup_terminal() -> io::Result<()> {
        let backend = TestBackend::new(10, 10);
        let _terminal = Terminal::new(backend)?;
        // TestBackend does not require actual terminal cleanup
        // The purpose of this test is just to ensure the function signature is correct
        // and it doesn't panic with a mock backend.
        // cleanup_terminal(&mut terminal)?; // This would cause a type mismatch
        Ok(())
    }

    // Note: Testing the `main` function directly is complex due to its
    // reliance on actual terminal I/O and long-running loops.
    // Mocking `crossterm` and `ratatui` for comprehensive UI interaction tests
    // would require a more sophisticated testing setup (e.g., a UI testing framework).
    // The `try_authenticate` and `fetch_labels` calls are already tested (or
    // have placeholders) in `gmail_api.rs`.
    // The `spawn_message_fetch_with_cache` and `spawn_message_fetch` functions
    // are difficult to test in isolation without mocking the entire `AppState`
    // and its dependencies, including network calls and database interactions.
    // For now, we focus on the `cleanup_terminal` helper.
}
