use crate::state::{AppState, ComposeField, FocusedPane};
use crate::types::LoadingStage;
use chrono::{DateTime, Local};
use ratatui::{prelude::*, widgets::*};

// Helper function to format email date
fn format_email_date(date_str: &str) -> String {
    if let Ok(dt_fixed) = DateTime::parse_from_rfc2822(date_str) {
        let dt_local = dt_fixed.with_timezone(&Local);
        let today = Local::now().date_naive();
        if dt_local.date_naive() == today {
            // If today, show only time in 5:55PM format
            dt_local.format("%-I:%M%P").to_string()
        } else {
            // If not today, show date in Dec 12, 2025 format
            dt_local.format("%b %-d, %Y").to_string()
        }
    } else {
        date_str.to_string()
    }
}

// Draw loading screen
pub fn draw_loading_screen(f: &mut ratatui::Frame, stage: &LoadingStage) {
    let area = f.size();

    let message = match stage {
        LoadingStage::Authenticating => "🔐 Authenticating with Gmail...",
        LoadingStage::FetchingLabels => "📁 Loading folders...",
    };

    let loading_text = Paragraph::new(message)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Gmail Terminal Client")
                .padding(Padding::uniform(1)),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: true });

    // Center the loading message
    let vertical_center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(5),
            Constraint::Percentage(55),
        ])
        .split(area);

    let horizontal_center = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(50),
            Constraint::Percentage(25),
        ])
        .split(vertical_center[1]);

    f.render_widget(loading_text, horizontal_center[1]);
}

pub fn draw_main_ui(f: &mut ratatui::Frame, state: &mut AppState) {
    // Update screen size for pagination calculations
    state.update_screen_size(f.size().height);

    // If there's an error message, draw it as a popup over everything else
    if state.error_message.is_some() {
        draw_error_popup(f, state);
        return; // Don't draw main UI if error popup is active
    }

    // Create main layout with optional help bar at bottom
    let main_chunks = if state.show_help {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(6)])
            .split(f.size())
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(100)])
            .split(f.size())
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(20),
                Constraint::Percentage(40),
                Constraint::Percentage(40),
            ]
            .as_ref(),
        )
        .split(main_chunks[0]);

    // Left: Folders
    let items: Vec<_> = state
        .labels
        .iter()
        .map(|l| ListItem::new(l.name.as_deref().unwrap_or("(unnamed)")))
        .collect();

    let folders_title = "Folders";

    let folders_border_style = if state.focused_pane == FocusedPane::Labels {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    let folders = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(folders_title)
                .border_style(folders_border_style)
                .padding(Padding::uniform(1)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(folders, chunks[0], &mut state.label_state);

    // Middle: Message list
    let msg_items: Vec<_> = if state.loading_messages && state.messages.is_empty() {
        // Only show loading if we have no cached messages to display
        vec![
            ListItem::new(""),
            ListItem::new("📧 Loading messages..."),
            ListItem::new("Please wait..."),
            ListItem::new(""),
        ]
    } else {
        state
            .messages
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let snippet = m.snippet.as_deref().unwrap_or("(no snippet)");
                let msg_id = m.id.as_deref().unwrap_or("");

                // Check if we have cached headers for this message
                if let Some((subject, from)) = state.message_headers.get(msg_id) {
                    // Check if we have a cached date for this message
                    let date_key = format!("{}_date", msg_id);
                    let formatted_date = state
                        .message_bodies
                        .get(&date_key)
                        .map(|s| format_email_date(s))
                        .unwrap_or_default();

                    // Calculate available width for the message pane (40% of screen width minus borders and padding)
                    // chunks[1].width includes the full column width
                    // Subtract 2 for left/right borders + 2 for left/right padding + 2 extra buffer = 6 total
                    let available_width = (chunks[1].width as usize).saturating_sub(6); // 2 for borders, 2 for padding, 2 for highlight symbol
                    let from_prefix = "From: ";
                    let from_text = format!("{}{}", from_prefix, from);

                    // Calculate spacing needed to right-align the date
                    let total_content_len = from_text.len() + formatted_date.len();
                    let spacing = if total_content_len < available_width {
                        available_width.saturating_sub(total_content_len)
                    } else {
                        1 // Minimum one space
                    };

                    let from_line =
                        format!("{}{}{}", from_text, " ".repeat(spacing), formatted_date);
                    ListItem::new(format!("{}\nSubject: {}", from_line, subject))
                } else {
                    ListItem::new(format!("#{}: {}", i + 1, snippet))
                }
            })
            .collect()
    };
    let messages_title = "Messages";

    let messages_border_style = if state.focused_pane == FocusedPane::Messages {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    let messages = List::new(msg_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(messages_title)
                .border_style(messages_border_style)
                .padding(Padding::uniform(1)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(messages, chunks[1], &mut state.message_state);

    // Right: Message detail with scrolling
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(0)]) // 6 lines for headers, rest for body
        .split(chunks[2]);

    let content_title = "Email Content";

    let content_border_style = if state.focused_pane == FocusedPane::Content {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    // Draw sticky header panel
    let header_block = Block::default()
        .borders(Borders::ALL)
        .title("Headers")
        .border_style(content_border_style);

    let header_text = if let Some(headers) = &state.current_message_display_headers {
        format!(
            "From: {}\nTo: {}\nDate: {}\nSubject: {}",
            headers.from,
            headers.to,
            format_email_date(&headers.date),
            headers.subject
        )
    } else {
        "No message selected or headers loaded.".to_string()
    };

    let header_paragraph = Paragraph::new(header_text)
        .block(header_block)
        .wrap(Wrap { trim: true });
    f.render_widget(header_paragraph, content_chunks[0]);

    // Draw message body
    let msg_body = if let Some(msg) = state.messages.get(state.selected_message) {
        let id = msg.id.as_deref().unwrap_or("");
        state
            .message_bodies
            .get(id)
            .map(|s| s.as_str())
            .unwrap_or("Press Enter to load message body...")
    } else {
        "No message selected"
    };

    // Apply scrolling by splitting content into lines and skipping based on scroll offset
    let lines: Vec<&str> = msg_body.lines().collect();
    let scrolled_content = if state.content_scroll_offset < lines.len() {
        lines[state.content_scroll_offset..].join("\n")
    } else {
        String::new()
    };

    let email = Paragraph::new(scrolled_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(content_title)
                .border_style(content_border_style)
                .padding(Padding::uniform(1)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(email, content_chunks[1]);

    // Status bar with key bindings (only show when help is enabled)
    if state.show_help {
        let help_text = match state.focused_pane {
            FocusedPane::Labels => vec![
                "j/k or ↑/↓: Navigate up/down through folders",
                "Enter: Select folder and switch to messages",
                "Tab/Shift+Tab: Switch panes | c: Compose email | f: Refresh messages",
                "Ctrl+R: Re-authenticate | ?: Toggle this help | q: Quit application",
            ]
            .join("\n"),
            FocusedPane::Messages => vec![
                "j/k or ↑/↓: Navigate up/down through messages",
                "Enter: View message content | c: Compose email | r: Reply to message",
                "a: Archive message | d: Delete message | f: Refresh messages",
                "Tab/Shift+Tab: Switch panes | Esc: Back to folders",
                "Ctrl+R: Re-authenticate | ?: Toggle this help | q: Quit application",
            ]
            .join("\n"),
            FocusedPane::Content => vec![
                "j/k or ↑/↓: Scroll up/down through content",
                "Tab/Shift+Tab: Switch panes | c: Compose email | r: Reply to message",
                "a: Archive message | d: Delete message | f: Refresh messages",
                "Esc: Back to folders pane",
                "Ctrl+R: Re-authenticate | ?: Toggle this help | q: Quit application",
            ]
            .join("\n"),
        };

        let status_bar = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Help (Press ? to toggle)")
                    .padding(Padding::uniform(1)),
            )
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        f.render_widget(status_bar, main_chunks[1]);
    }
}

// Draw error popup
pub fn draw_error_popup(f: &mut ratatui::Frame, state: &mut AppState) {
    if let Some(error_msg) = &state.error_message {
        let area = f.size();
        let popup_area = centered_rect(60, 20, area); // 60% width, 20% height

        f.render_widget(Clear, popup_area); // Clear the area first

        let block = Block::default()
            .title("Error")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));

        let paragraph = Paragraph::new(error_msg.clone())
            .block(block)
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, popup_area);
    }
}

pub fn draw_compose_ui(f: &mut ratatui::Frame, state: &mut AppState) {
    let area = f.size();

    // Create a centered popup with consistent height
    let popup_area = centered_rect(80, 85, area);

    // Clear the background
    f.render_widget(Clear, popup_area);

    // Main compose window
    let compose_block = Block::default()
        .borders(Borders::ALL)
        .title("Compose Email")
        .border_style(Style::default().fg(Color::Blue));
    f.render_widget(compose_block, popup_area);

    // Create layout for form fields
    let inner_area = popup_area.inner(&Margin {
        horizontal: 1,
        vertical: 1,
    });

    // Always allocate space for all fields to maintain consistent layout
    let mut constraints = vec![
        Constraint::Length(3), // To - single line height
        Constraint::Length(3), // Cc - single line height
    ];

    if state.compose_state.show_bcc {
        constraints.push(Constraint::Length(3)); // Bcc - single line height (always allocated, but may not be rendered)
    }

    constraints.extend_from_slice(&[
        Constraint::Length(3), // Subject - single line height
        Constraint::Min(8),    // Body
        Constraint::Length(3), // Send button
    ]);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    let mut chunk_idx = 0;

    // To field
    let to_style = if state.compose_state.focused_field == ComposeField::To {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let to_field = Paragraph::new(state.compose_state.to.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("To:")
                .border_style(to_style),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(to_field, chunks[chunk_idx]);
    if state.compose_state.focused_field == ComposeField::To {
        f.set_cursor(
            chunks[chunk_idx].x + 1 + state.compose_state.to_cursor_position as u16,
            chunks[chunk_idx].y + 1,
        );
    }
    chunk_idx += 1;

    // Cc field
    let cc_style = if state.compose_state.focused_field == ComposeField::Cc {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let cc_field = Paragraph::new(state.compose_state.cc.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Cc:")
                .border_style(cc_style),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(cc_field, chunks[chunk_idx]);
    if state.compose_state.focused_field == ComposeField::Cc {
        f.set_cursor(
            chunks[chunk_idx].x + 1 + state.compose_state.cc_cursor_position as u16,
            chunks[chunk_idx].y + 1,
        );
    }
    chunk_idx += 1;

    // Bcc field (always has allocated space, but only rendered if shown)
    if state.compose_state.show_bcc {
        let bcc_style = if state.compose_state.focused_field == ComposeField::Bcc {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let bcc_field = Paragraph::new(state.compose_state.bcc.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Bcc:")
                    .border_style(bcc_style),
            )
            .wrap(Wrap { trim: true });
        f.render_widget(bcc_field, chunks[chunk_idx]);
        if state.compose_state.focused_field == ComposeField::Bcc {
            f.set_cursor(
                chunks[chunk_idx].x + 1 + state.compose_state.bcc_cursor_position as u16,
                chunks[chunk_idx].y + 1,
            );
        }
        // Always increment chunk_idx to account for BCC space
        chunk_idx += 1;
    }

    // Subject field
    let subject_style = if state.compose_state.focused_field == ComposeField::Subject {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let subject_field = Paragraph::new(state.compose_state.subject.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Subject:")
                .border_style(subject_style),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(subject_field, chunks[chunk_idx]);
    if state.compose_state.focused_field == ComposeField::Subject {
        f.set_cursor(
            chunks[chunk_idx].x + 1 + state.compose_state.subject_cursor_position as u16,
            chunks[chunk_idx].y + 1,
        );
    }
    chunk_idx += 1;

    // Body field
    let body_style = if state.compose_state.focused_field == ComposeField::Body {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let body_field = Paragraph::new(state.compose_state.body.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Body:")
                .border_style(body_style),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(body_field, chunks[chunk_idx]);
    if state.compose_state.focused_field == ComposeField::Body {
        // For the body field, we need to calculate the cursor position based on lines and scroll offset
        let text = state.compose_state.body.as_str();
        let lines: Vec<&str> = text.lines().collect();
        let cursor_pos = state.compose_state.body_cursor_position;

        let mut current_line_idx = 0;
        let mut chars_on_current_line = 0;

        for (i, line) in lines.iter().enumerate() {
            if cursor_pos <= chars_on_current_line + line.len() {
                current_line_idx = i;
                break;
            }
            chars_on_current_line += line.len() + 1; // +1 for newline character
        }

        let x_offset = cursor_pos.saturating_sub(chars_on_current_line);
        let y_offset = current_line_idx;

        f.set_cursor(
            chunks[chunk_idx].x + 1 + x_offset as u16,
            chunks[chunk_idx].y + 1 + y_offset as u16,
        );
    }
    chunk_idx += 1;

    // Send button
    let send_style = if state.compose_state.focused_field == ComposeField::Send {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let send_text = if state.compose_state.sending {
        "Sending..."
    } else {
        "[ Send Email ]"
    };
    let send_button = Paragraph::new(send_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(send_style),
        )
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(send_button, chunks[chunk_idx]);

    // Help text at bottom
    let help_text =
        "Tab/Shift+Tab: Navigate | Ctrl+B: Toggle Bcc | Enter: Send (on Send button) | Esc: Cancel";
    let help_area = Rect {
        x: popup_area.x,
        y: popup_area.y + popup_area.height,
        width: popup_area.width,
        height: 1,
    };
    if help_area.y < area.height {
        let help_paragraph = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(help_paragraph, help_area);
    }
}

// Helper function to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
