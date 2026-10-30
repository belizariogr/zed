//! Indentation for the line created by pressing enter, following the algorithm VS Code
//! uses for its `onEnterRules` and `indentationRules` language configuration
//! (`getEnterAction` and `getIndentForEnter` in VS Code's editor). Tree-sitter stands in
//! for VS Code's TextMate tokens: it tells which brackets are inside strings or comments,
//! and which positions belong to an embedded scope such as a JSX tag.

use crate::{BufferSnapshot, IndentationRules, Language, OnEnterConfig, OnEnterIndent};
use std::{num::NonZeroU32, ops::Range, sync::Arc};
use text::Point;
use util::ResultExt as _;

/// The edit that pressing enter makes over a range of a buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnEnterEdit {
    /// Text replacing the range, starting with a newline.
    pub text: String,
    /// The end of the replaced range. It can extend past whitespace after the range,
    /// which the new indentation replaces.
    pub end: Point,
    /// Whether the text after the range moved to a second new line, which leaves the
    /// cursor at the end of the first new line.
    pub has_extra_line: bool,
    /// How many columns before the end of the text the cursor ends up. Pressing enter
    /// inside the indentation of a line keeps the cursor at the same column instead of
    /// moving it after the new indentation.
    pub cursor_columns_before_end: u32,
}

#[derive(Clone, Copy)]
struct IndentStyle {
    tab_size: u32,
    insert_spaces: bool,
}

#[derive(Clone)]
struct Scope {
    language: Arc<Language>,
    embedded_ix: Option<usize>,
}

impl PartialEq for Scope {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.language, &other.language) && self.embedded_ix == other.embedded_ix
    }
}

impl Scope {
    fn config(&self) -> Option<&OnEnterConfig> {
        let config = self.language.config.on_enter.as_ref()?;
        match self.embedded_ix {
            Some(ix) => config.embedded.get(ix).map(|embedded| &embedded.config),
            None => Some(config),
        }
    }
}

struct EnterAction<'a> {
    indent: OnEnterIndent,
    append_text: Option<&'a str>,
    remove_text: usize,
}

impl BufferSnapshot {
    /// Computes the edit for pressing enter over `range`, or `None` if the language at
    /// the range has no [`OnEnterConfig`].
    pub fn on_enter_edit(
        &self,
        range: Range<Point>,
        tab_size: NonZeroU32,
        hard_tabs: bool,
    ) -> Option<OnEnterEdit> {
        let style = IndentStyle {
            tab_size: tab_size.get(),
            insert_spaces: !hard_tabs,
        };
        let start = range.start;
        let cursor_scope = self.scope_at_position(start);
        let config = cursor_scope.config()?;

        let line_start_scope = self.scope_at_line_start(start.row);
        let start_run = self.scope_run_start(start, &cursor_scope);
        let language_differs_from_line_start = line_start_scope != cursor_scope;
        let before_start = if language_differs_from_line_start {
            start_run
        } else {
            0
        };
        let before_text = self.processed_text(start.row, before_start..start.column, config);

        let end = range.end;
        let end_scope = self.scope_at_position(end);
        let after_end = self.scope_run_end(end, &end_scope);
        let after_text = self.processed_text(end.row, end.column..after_end, config);

        let previous_line_text = if start_run == 0 && start.row > 0 {
            let previous_row = start.row - 1;
            let previous_line_end = Point::new(previous_row, self.line_len(previous_row));
            let previous_scope = self.scope_at_position(previous_line_end);
            if previous_scope == cursor_scope {
                let run_start = self.scope_run_start(previous_line_end, &previous_scope);
                self.processed_text(previous_row, run_start..previous_line_end.column, config)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let current_line_indent = self.leading_whitespace(start.row);
        let indentation =
            &current_line_indent[..current_line_indent.len().min(start.column as usize)];

        if let Some(action) = enter_action(config, &previous_line_text, &before_text, &after_text) {
            let mut indentation = indentation.to_string();
            indentation.truncate(indentation.len().saturating_sub(action.remove_text));
            let append_text = match (action.append_text, action.indent) {
                (None, OnEnterIndent::Indent | OnEnterIndent::IndentOutdent) => "\t".to_string(),
                (None, _) => String::new(),
                (Some(text), OnEnterIndent::Indent) => format!("\t{text}"),
                (Some(text), _) => text.to_string(),
            };
            let (text, has_extra_line) = match action.indent {
                OnEnterIndent::None | OnEnterIndent::Indent => (
                    format!(
                        "\n{}",
                        normalize(&format!("{indentation}{append_text}"), style)
                    ),
                    false,
                ),
                OnEnterIndent::IndentOutdent => (
                    format!(
                        "\n{}\n{}",
                        normalize(&format!("{indentation}{append_text}"), style),
                        normalize(&indentation, style)
                    ),
                    true,
                ),
                OnEnterIndent::Outdent => (
                    format!(
                        "\n{}",
                        normalize(
                            &format!("{}{append_text}", unshift(&indentation, style)),
                            style
                        )
                    ),
                    false,
                ),
            };
            return Some(OnEnterEdit {
                text,
                end,
                has_extra_line,
                cursor_columns_before_end: 0,
            });
        }

        if let Some(rules) = &config.indentation_rules {
            let before_indent = leading_whitespace(&before_text);
            let target_row = (start.row + 1).min(self.max_point().row);
            let target_scope = self.scope_at_line_start(target_row);
            let inherited =
                self.inherited_indent(start.row, &before_text, rules, config, &target_scope);
            let after_enter = match inherited {
                None if language_differs_from_line_start => current_line_indent.clone(),
                None => before_indent.to_string(),
                Some((indentation, should_indent)) => {
                    let mut after_enter = if language_differs_from_line_start {
                        current_line_indent.clone()
                    } else {
                        indentation
                    };
                    if should_indent {
                        after_enter = shift(&after_enter, style);
                    }
                    if matches(&rules.decrease_indent_pattern, &after_text) {
                        after_enter = unshift(&after_enter, style);
                    }
                    after_enter
                }
            };

            let after_enter = normalize(&after_enter, style);
            let end_line_indent = self.leading_whitespace(end.row);
            let end_line_indent_len = end_line_indent.len() as u32;
            let end_line_has_text = end_line_indent_len < self.line_len(end.row);
            let end_column = if end_line_has_text {
                end.column.max(end_line_indent_len)
            } else {
                self.line_len(end.row)
            };
            let cursor_columns_before_end =
                if end_line_has_text && end.column <= end_line_indent_len {
                    let mut cursor_column =
                        visible_width(&end_line_indent[..end.column as usize], style.tab_size);
                    if !style.insert_spaces {
                        cursor_column = cursor_column.div_ceil(style.tab_size);
                    }
                    (after_enter.len() as u32).saturating_sub(cursor_column)
                } else {
                    0
                };
            return Some(OnEnterEdit {
                text: format!("\n{after_enter}"),
                end: Point::new(end.row, end_column),
                has_extra_line: false,
                cursor_columns_before_end,
            });
        }

        Some(OnEnterEdit {
            text: format!("\n{}", normalize(indentation, style)),
            end,
            has_extra_line: false,
            cursor_columns_before_end: 0,
        })
    }

    /// Indentation of the line after `cursor_row` based on the lines above it, and
    /// whether it should be indented one more level. `cursor_line` replaces the text of
    /// `cursor_row`, which ends at the cursor.
    fn inherited_indent(
        &self,
        cursor_row: u32,
        cursor_line: &str,
        rules: &IndentationRules,
        config: &OnEnterConfig,
        target_scope: &Scope,
    ) -> Option<(String, bool)> {
        let line = |row: u32| -> String {
            if row == cursor_row {
                cursor_line.to_string()
            } else {
                self.text_for_range(Point::new(row, 0)..Point::new(row, self.line_len(row)))
                    .collect()
            }
        };
        let processed_line = |row: u32| -> String {
            if row == cursor_row {
                cursor_line.to_string()
            } else {
                self.processed_text(row, 0..self.line_len(row), config)
            }
        };

        let target_row = cursor_row + 1;
        for row in (0..target_row).rev() {
            if !line(row).is_empty() {
                break;
            }
            if row == 0 {
                return Some((String::new(), false));
            }
        }

        let mut preceding_row = None;
        let mut skipped_row = None;
        for row in (0..target_row).rev() {
            if self.scope_at_line_start(row) != *target_scope {
                preceding_row = skipped_row;
                break;
            }
            let text = line(row);
            if text.trim().is_empty()
                || matches(&rules.unindented_line_pattern, &processed_line(row))
            {
                skipped_row = Some(row);
                continue;
            }
            preceding_row = Some(row);
            break;
        }
        let preceding_row = preceding_row?;
        let preceding_line = processed_line(preceding_row);
        let preceding_indent = leading_whitespace(&line(preceding_row)).to_string();

        if matches(&rules.increase_indent_pattern, &preceding_line)
            || matches(&rules.indent_next_line_pattern, &preceding_line)
        {
            return Some((preceding_indent, true));
        }
        if matches(&rules.decrease_indent_pattern, &preceding_line) || preceding_row == 0 {
            return Some((preceding_indent, false));
        }

        // A line below one that only indents its next line, like the body of a braceless
        // `if`, has a temporary indentation. Take it from the line that started the chain.
        let previous_row = preceding_row - 1;
        let previous_line = line(previous_row);
        if !matches(&rules.increase_indent_pattern, &previous_line)
            && !matches(&rules.decrease_indent_pattern, &previous_line)
            && matches(&rules.indent_next_line_pattern, &previous_line)
        {
            let chain_start_row = (0..previous_row)
                .rev()
                .find(|row| !matches(&rules.indent_next_line_pattern, &processed_line(*row)))
                .map_or(0, |row| row + 1);
            return Some((
                leading_whitespace(&line(chain_start_row)).to_string(),
                false,
            ));
        }

        Some((preceding_indent, false))
    }

    /// The line's text in `columns`, without the brackets inside strings and comments.
    fn processed_text(&self, row: u32, columns: Range<u32>, config: &OnEnterConfig) -> String {
        let text: String = self
            .text_for_range(Point::new(row, columns.start)..Point::new(row, columns.end))
            .collect();
        let mut brackets = config
            .brackets
            .iter()
            .flatten()
            .filter(|bracket| !bracket.is_empty())
            .collect::<Vec<_>>();
        if brackets.is_empty() {
            return text;
        }
        brackets.sort_by_key(|bracket| std::cmp::Reverse(bracket.len()));

        let text_start = self.point_to_offset(Point::new(row, columns.start));
        let mut result = String::with_capacity(text.len());
        let mut ix = 0;
        while let Some(rest) = text.get(ix..)
            && let Some(character) = rest.chars().next()
        {
            if let Some(bracket) = brackets
                .iter()
                .find(|bracket| rest.starts_with(bracket.as_str()))
            {
                let is_in_string_or_comment =
                    self.language_scope_at(text_start + ix)
                        .is_some_and(|scope| {
                            matches!(scope.override_name(), Some("string" | "comment"))
                        });
                if !is_in_string_or_comment {
                    result.push_str(bracket);
                }
                ix += bracket.len();
            } else {
                result.push(character);
                ix += character.len_utf8();
            }
        }
        result
    }

    fn leading_whitespace(&self, row: u32) -> String {
        self.chars_at(Point::new(row, 0))
            .take_while(|character| *character == ' ' || *character == '\t')
            .collect()
    }

    /// The scope VS Code assigns to a position: the one of the character after it, or
    /// of the character before it at the end of a line.
    fn scope_at_position(&self, position: Point) -> Scope {
        let line_len = self.line_len(position.row);
        if line_len == 0 {
            return self.scope_at_offset(self.point_to_offset(position), 0);
        }
        let offset = self.point_to_offset(position);
        if position.column < line_len {
            let character_len = self.chars_at(offset).next().map_or(0, char::len_utf8);
            self.scope_at_offset(offset, character_len)
        } else {
            let character_len = self
                .reversed_chars_at(offset)
                .next()
                .map_or(0, char::len_utf8);
            self.scope_at_offset(offset - character_len, character_len)
        }
    }

    fn scope_at_line_start(&self, row: u32) -> Scope {
        let offset = self.point_to_offset(Point::new(row, 0));
        let character_len = if self.line_len(row) == 0 {
            0
        } else {
            self.chars_at(offset).next().map_or(0, char::len_utf8)
        };
        self.scope_at_offset(offset, character_len)
    }

    fn scope_at_offset(&self, offset: usize, len: usize) -> Scope {
        let layer = self.syntax_layer_at(offset);
        let Some(language) = layer
            .as_ref()
            .map(|layer| layer.language.clone())
            .or_else(|| self.language().cloned())
        else {
            return Scope {
                language: crate::PLAIN_TEXT.clone(),
                embedded_ix: None,
            };
        };
        let embedded_ix = maybe_embedded_ix(&language, layer.as_ref(), offset, len);
        Scope {
            language,
            embedded_ix,
        }
    }

    /// The column where the run of characters with `scope` containing `position` starts.
    fn scope_run_start(&self, position: Point, scope: &Scope) -> u32 {
        let line_len = self.line_len(position.row);
        if line_len == 0 || self.line_is_single_scope(position.row) {
            return 0;
        }
        let line_start = self.point_to_offset(Point::new(position.row, 0));
        let mut offset = self.point_to_offset(position);
        if position.column >= line_len {
            offset -= self
                .reversed_chars_at(offset)
                .next()
                .map_or(0, char::len_utf8);
        }
        for character in self.reversed_chars_at(offset) {
            if offset == line_start {
                break;
            }
            let character_len = character.len_utf8();
            if self.scope_at_offset(offset - character_len, character_len) != *scope {
                break;
            }
            offset -= character_len;
        }
        (offset - line_start) as u32
    }

    /// The column where the run of characters with `scope` containing `position` ends.
    fn scope_run_end(&self, position: Point, scope: &Scope) -> u32 {
        let line_len = self.line_len(position.row);
        if self.line_is_single_scope(position.row) {
            return line_len;
        }
        let line_end = self.point_to_offset(Point::new(position.row, line_len));
        let mut offset = self.point_to_offset(position);
        for character in self.chars_at(offset) {
            if offset >= line_end {
                break;
            }
            let character_len = character.len_utf8();
            if self.scope_at_offset(offset, character_len) != *scope {
                break;
            }
            offset += character_len;
        }
        line_len - (line_end - offset) as u32
    }

    fn line_is_single_scope(&self, row: u32) -> bool {
        let range = Point::new(row, 0)..Point::new(row, self.line_len(row));
        let mut layers = self.syntax_layers_for_range(range, false);
        let Some(layer) = layers.next() else {
            return true;
        };
        layers.next().is_none()
            && layer
                .language
                .config
                .on_enter
                .as_ref()
                .is_none_or(|config| config.embedded.is_empty())
    }
}

fn maybe_embedded_ix(
    language: &Language,
    layer: Option<&crate::SyntaxLayer>,
    offset: usize,
    len: usize,
) -> Option<usize> {
    let config = language.config.on_enter.as_ref()?;
    if config.embedded.is_empty() {
        return None;
    }
    let mut node = layer?
        .node()
        .descendant_for_byte_range(offset, offset + len)?;
    loop {
        let kind = node.kind();
        for (ix, embedded) in config.embedded.iter().enumerate() {
            if embedded
                .exit_node_kinds
                .iter()
                .any(|exit_kind| exit_kind == kind)
            {
                return None;
            }
            if embedded
                .node_kinds
                .iter()
                .any(|node_kind| node_kind == kind)
            {
                return Some(ix);
            }
        }
        node = node.parent()?;
    }
}

fn enter_action<'a>(
    config: &'a OnEnterConfig,
    previous_line_text: &str,
    before_text: &str,
    after_text: &str,
) -> Option<EnterAction<'a>> {
    for rule in &config.rules {
        if rule
            .before_text
            .is_match(before_text)
            .log_err()
            .unwrap_or(false)
            && (rule.after_text.is_none() || matches(&rule.after_text, after_text))
            && (rule.previous_line_text.is_none()
                || matches(&rule.previous_line_text, previous_line_text))
        {
            return Some(EnterAction {
                indent: rule.indent,
                append_text: rule.append_text.as_deref(),
                remove_text: rule.remove_text,
            });
        }
    }

    let bracket_action = |indent| EnterAction {
        indent,
        append_text: None,
        remove_text: 0,
    };
    if !before_text.is_empty() && !after_text.is_empty() {
        let is_between_brackets = config.brackets.iter().any(|[open, close]| {
            ends_with_open_bracket(before_text, open)
                && starts_with_close_bracket(after_text, close)
        });
        if is_between_brackets {
            return Some(bracket_action(OnEnterIndent::IndentOutdent));
        }
    }
    if !before_text.is_empty()
        && config
            .brackets
            .iter()
            .any(|[open, _]| ends_with_open_bracket(before_text, open))
    {
        return Some(bracket_action(OnEnterIndent::Indent));
    }
    None
}

fn matches(regex: &Option<fancy_regex::Regex>, text: &str) -> bool {
    regex
        .as_ref()
        .is_some_and(|regex| regex.is_match(text).log_err().unwrap_or(false))
}

fn is_word_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// Matches VS Code's `\b{open}\s*$`, where `\b` is only added for brackets starting with
/// a word character.
fn ends_with_open_bracket(text: &str, open: &str) -> bool {
    let Some(prefix) = text.trim_end().strip_suffix(open) else {
        return false;
    };
    !open.starts_with(is_word_character) || !prefix.ends_with(is_word_character)
}

/// Matches VS Code's `^\s*{close}\b`, where `\b` is only added for brackets ending with
/// a word character.
fn starts_with_close_bracket(text: &str, close: &str) -> bool {
    let Some(suffix) = text.trim_start().strip_prefix(close) else {
        return false;
    };
    !close.ends_with(is_word_character) || !suffix.starts_with(is_word_character)
}

fn leading_whitespace(text: &str) -> &str {
    let len = text
        .find(|character| character != ' ' && character != '\t')
        .unwrap_or(text.len());
    &text[..len]
}

fn visible_width(indentation: &str, tab_size: u32) -> u32 {
    indentation.chars().fold(0, |width, character| {
        if character == '\t' {
            width + tab_size - width % tab_size
        } else {
            width + 1
        }
    })
}

fn indentation_of_width(width: u32, style: IndentStyle) -> String {
    if style.insert_spaces {
        " ".repeat(width as usize)
    } else {
        let mut indentation = "\t".repeat((width / style.tab_size) as usize);
        indentation.push_str(&" ".repeat((width % style.tab_size) as usize));
        indentation
    }
}

/// Converts the leading whitespace of `text` to the indentation style of the buffer.
fn normalize(text: &str, style: IndentStyle) -> String {
    let indentation = leading_whitespace(text);
    let width = visible_width(indentation, style.tab_size);
    let mut result = indentation_of_width(width, style);
    result.push_str(&text[indentation.len()..]);
    result
}

/// Indents to the next tab stop.
fn shift(indentation: &str, style: IndentStyle) -> String {
    let width = visible_width(indentation, style.tab_size);
    indentation_of_width(width + style.tab_size - width % style.tab_size, style)
}

/// Outdents to the previous tab stop.
fn unshift(indentation: &str, style: IndentStyle) -> String {
    let width = visible_width(indentation, style.tab_size);
    let width = width.saturating_sub(1);
    indentation_of_width(width - width % style.tab_size, style)
}
