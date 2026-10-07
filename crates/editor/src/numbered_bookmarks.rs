use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use collections::HashMap;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, Hsla,
    Keystroke, Modifiers, MouseButton, Rgba, TaskExt, Window, div, px,
};
use multi_buffer::ToPoint as _;
use project::bookmark_store::SerializedNumberedBookmark;
use rope::Point;
use settings::{NumberedBookmarkNavigateThroughAllFiles, NumberedBookmarkRevealLocation, Settings};
use text::Bias;
use ui::{
    InteractiveElement as _, ListItem, ListItemSpacing, ParentElement as _,
    StatefulInteractiveElement as _, Styled, prelude::*,
};
use workspace::{
    DismissDecision, ModalView, OpenOptions, OpenVisible, Toast, Workspace,
    notifications::NotificationId,
};

use crate::display_map::{DisplayRow, ToDisplayPoint};
use crate::{
    ClearNumberedBookmarks, ClearNumberedBookmarksFromAllFiles, Editor, EditorSettings,
    JumpToNumberedBookmark0, JumpToNumberedBookmark1, JumpToNumberedBookmark2,
    JumpToNumberedBookmark3, JumpToNumberedBookmark4, JumpToNumberedBookmark5,
    JumpToNumberedBookmark6, JumpToNumberedBookmark7, JumpToNumberedBookmark8,
    JumpToNumberedBookmark9, ListNumberedBookmarks, ListNumberedBookmarksFromAllFiles,
    SelectionEffects, ToggleNumberedBookmark0, ToggleNumberedBookmark1, ToggleNumberedBookmark2,
    ToggleNumberedBookmark3, ToggleNumberedBookmark4, ToggleNumberedBookmark5,
    ToggleNumberedBookmark6, ToggleNumberedBookmark7, ToggleNumberedBookmark8,
    ToggleNumberedBookmark9, scroll::Autoscroll,
};

pub(crate) enum NumberedBookmarkRowHighlights {}

struct NumberedBookmarkNotDefined;
struct NoNumberedBookmarksFound;

#[derive(Clone)]
struct NumberedBookmarkListItem {
    path: Arc<Path>,
    number: u8,
    row: u32,
    column: u32,
    line_text: String,
    in_active_file: bool,
}

pub(crate) struct NumberedBookmarkList {
    picker_editor: Entity<Editor>,
    active_editor: Entity<Editor>,
    items: Vec<NumberedBookmarkListItem>,
    matches: Vec<usize>,
    selected_index: usize,
    previous_selection: Range<Point>,
    confirmed: bool,
    all_files: bool,
}

impl EventEmitter<DismissEvent> for NumberedBookmarkList {}

impl Focusable for NumberedBookmarkList {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker_editor.focus_handle(cx)
    }
}

impl ModalView for NumberedBookmarkList {
    fn on_before_dismiss(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DismissDecision {
        if !self.confirmed {
            let previous_selection = self.previous_selection.clone();
            self.active_editor.update(cx, |editor, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_ranges(std::iter::once(
                        previous_selection.start..previous_selection.end,
                    ));
                });
            });
        }
        DismissDecision::Dismiss(true)
    }
}

impl Editor {
    pub(crate) fn numbered_bookmark_hotkeys(&self, cx: &mut App) -> Vec<Keystroke> {
        let Some(store) = self.bookmark_store.as_ref() else {
            return Vec::new();
        };
        let buffers = self.buffer.read(cx).all_buffers();
        store.update(cx, |store, cx| {
            (0..=9)
                .filter(|number| {
                    buffers.iter().any(|buffer| {
                        store
                            .numbered_bookmark_in_buffer(buffer, *number, cx)
                            .is_some()
                    })
                })
                .map(|number| Keystroke {
                    modifiers: Modifiers {
                        platform: true,
                        ..Modifiers::none()
                    },
                    key: number.to_string(),
                    key_char: None,
                })
                .collect()
        })
    }

    pub(crate) fn refresh_numbered_bookmark_hotkeys(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cfg!(target_os = "macos") || self.bookmark_store.is_none() {
            return;
        }
        if let Some(workspace) = self.workspace() {
            let active_editor = workspace.read(cx).active_item(cx).and_then(|item| {
                if item.item_id() == cx.entity_id() {
                    Some(cx.entity_id())
                } else {
                    item.act_as::<Editor>(cx).map(|editor| editor.entity_id())
                }
            });
            if active_editor.is_none() {
                window.set_system_hotkeys_to_suppress(&[]);
                return;
            }
            if active_editor != Some(cx.entity_id()) {
                return;
            }
        } else if !self.is_focused(window) {
            return;
        }
        window.set_system_hotkeys_to_suppress(&self.numbered_bookmark_hotkeys(cx));
    }

    pub fn toggle_numbered_bookmark_0(
        &mut self,
        _: &ToggleNumberedBookmark0,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(0, window, cx);
    }

    pub fn toggle_numbered_bookmark_1(
        &mut self,
        _: &ToggleNumberedBookmark1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(1, window, cx);
    }

    pub fn toggle_numbered_bookmark_2(
        &mut self,
        _: &ToggleNumberedBookmark2,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(2, window, cx);
    }

    pub fn toggle_numbered_bookmark_3(
        &mut self,
        _: &ToggleNumberedBookmark3,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(3, window, cx);
    }

    pub fn toggle_numbered_bookmark_4(
        &mut self,
        _: &ToggleNumberedBookmark4,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(4, window, cx);
    }

    pub fn toggle_numbered_bookmark_5(
        &mut self,
        _: &ToggleNumberedBookmark5,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(5, window, cx);
    }

    pub fn toggle_numbered_bookmark_6(
        &mut self,
        _: &ToggleNumberedBookmark6,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(6, window, cx);
    }

    pub fn toggle_numbered_bookmark_7(
        &mut self,
        _: &ToggleNumberedBookmark7,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(7, window, cx);
    }

    pub fn toggle_numbered_bookmark_8(
        &mut self,
        _: &ToggleNumberedBookmark8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(8, window, cx);
    }

    pub fn toggle_numbered_bookmark_9(
        &mut self,
        _: &ToggleNumberedBookmark9,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_numbered_bookmark(9, window, cx);
    }

    pub fn jump_to_numbered_bookmark_0(
        &mut self,
        _: &JumpToNumberedBookmark0,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(0, window, cx);
    }

    pub fn jump_to_numbered_bookmark_1(
        &mut self,
        _: &JumpToNumberedBookmark1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(1, window, cx);
    }

    pub fn jump_to_numbered_bookmark_2(
        &mut self,
        _: &JumpToNumberedBookmark2,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(2, window, cx);
    }

    pub fn jump_to_numbered_bookmark_3(
        &mut self,
        _: &JumpToNumberedBookmark3,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(3, window, cx);
    }

    pub fn jump_to_numbered_bookmark_4(
        &mut self,
        _: &JumpToNumberedBookmark4,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(4, window, cx);
    }

    pub fn jump_to_numbered_bookmark_5(
        &mut self,
        _: &JumpToNumberedBookmark5,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(5, window, cx);
    }

    pub fn jump_to_numbered_bookmark_6(
        &mut self,
        _: &JumpToNumberedBookmark6,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(6, window, cx);
    }

    pub fn jump_to_numbered_bookmark_7(
        &mut self,
        _: &JumpToNumberedBookmark7,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(7, window, cx);
    }

    pub fn jump_to_numbered_bookmark_8(
        &mut self,
        _: &JumpToNumberedBookmark8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(8, window, cx);
    }

    pub fn jump_to_numbered_bookmark_9(
        &mut self,
        _: &JumpToNumberedBookmark9,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_numbered_bookmark(9, window, cx);
    }

    pub fn list_numbered_bookmarks(
        &mut self,
        _: &ListNumberedBookmarks,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_numbered_bookmark_list(false, window, cx);
    }

    pub fn list_numbered_bookmarks_from_all_files(
        workspace: &mut Workspace,
        _: &ListNumberedBookmarksFromAllFiles,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let Some(editor) = workspace
            .active_item(cx)
            .and_then(|item| item.act_as::<Editor>(cx))
        else {
            return;
        };
        editor.update(cx, |editor, cx| {
            editor.show_numbered_bookmark_list(true, window, cx);
        });
    }

    pub fn clear_numbered_bookmarks(
        &mut self,
        _: &ClearNumberedBookmarks,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bookmark_store) = self.bookmark_store.clone() else {
            return;
        };
        let Some(buffer) = self.buffer.read(cx).as_singleton() else {
            return;
        };
        bookmark_store.update(cx, |store, cx| {
            store.clear_numbered_bookmarks_for_buffer(&buffer, cx);
        });
        self.persist_numbered_bookmarks_project_file(cx);
        self.refresh_numbered_bookmark_highlights(cx);
    }

    pub fn clear_numbered_bookmarks_from_all_files(
        workspace: &mut Workspace,
        _: &ClearNumberedBookmarksFromAllFiles,
        _: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        workspace
            .project()
            .read(cx)
            .bookmark_store()
            .update(cx, |store, cx| {
                store.clear_all_numbered_bookmarks(cx);
            });
        if EditorSettings::get_global(cx)
            .numbered_bookmarks
            .save_bookmarks_in_project
        {
            let project = workspace.project().clone();
            let fs = project.read(cx).fs().clone();
            project
                .read(cx)
                .bookmark_store()
                .read(cx)
                .persist_vscode_project_files(fs, cx)
                .detach_and_log_err(cx);
        }
    }

    fn toggle_numbered_bookmark(
        &mut self,
        number: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot(window, cx);
        let selection = self
            .selections
            .newest::<Point>(&snapshot.display_snapshot)
            .head();
        self.toggle_numbered_bookmark_at_point(number, selection, cx);
    }

    fn toggle_numbered_bookmark_at_row(
        &mut self,
        number: u8,
        row: DisplayRow,
        cx: &mut Context<Self>,
    ) {
        let display_snapshot = self.display_snapshot(cx);
        let point = display_snapshot.display_point_to_point(row.as_display_point(), Bias::Left);
        self.toggle_numbered_bookmark_at_point(number, point, cx);
    }

    fn toggle_numbered_bookmark_at_point(
        &mut self,
        number: u8,
        point: Point,
        cx: &mut Context<Self>,
    ) {
        let Some(bookmark_store) = self.bookmark_store.clone() else {
            return;
        };
        let Some(project) = self.project() else {
            return;
        };

        let multi_buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let multibuffer_anchor = multi_buffer_snapshot.anchor_before(point);
        let Some((buffer_anchor, _)) =
            multi_buffer_snapshot.anchor_to_buffer_anchor(multibuffer_anchor)
        else {
            return;
        };
        let Some(buffer) = project.read(cx).buffer_for_id(buffer_anchor.buffer_id, cx) else {
            return;
        };

        let unique_across_files = matches!(
            EditorSettings::get_global(cx)
                .numbered_bookmarks
                .navigate_through_all_files,
            NumberedBookmarkNavigateThroughAllFiles::Replace
        );

        bookmark_store.update(cx, |store, cx| {
            store.toggle_numbered_bookmark(buffer, buffer_anchor, number, unique_across_files, cx);
        });
        self.persist_numbered_bookmarks_project_file(cx);
        self.refresh_numbered_bookmark_highlights(cx);
    }

    fn jump_to_numbered_bookmark(
        &mut self,
        number: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bookmark_store) = self.bookmark_store.clone() else {
            return;
        };
        let Some(current_path) = self.buffer.read(cx).as_singleton().and_then(|buffer| {
            project::bookmark_store::BookmarkStore::abs_path_from_buffer(&buffer, cx)
        }) else {
            self.warn_numbered_bookmark_not_defined(number, cx);
            return;
        };

        let navigate = EditorSettings::get_global(cx)
            .numbered_bookmarks
            .navigate_through_all_files;
        let snapshot = self.snapshot(window, cx);
        let current_row = self
            .selections
            .newest::<Point>(&snapshot.display_snapshot)
            .head()
            .row;

        let locations = bookmark_store.read(cx).numbered_bookmark_locations(cx);
        let current_file_location = locations.iter().find(|(path, bookmark)| {
            path.as_ref() == current_path.as_ref() && bookmark.number == number
        });

        match navigate {
            NumberedBookmarkNavigateThroughAllFiles::Disabled => {
                if let Some((_, bookmark)) = current_file_location {
                    self.reveal_numbered_bookmark_in_current_file(bookmark, window, cx);
                } else if bookmark_store.read(cx).has_numbered_bookmarks() {
                    self.warn_numbered_bookmark_not_defined(number, cx);
                } else {
                    self.notify_no_numbered_bookmarks(cx);
                }
            }
            NumberedBookmarkNavigateThroughAllFiles::Replace => {
                if let Some((_, bookmark)) = current_file_location {
                    self.reveal_numbered_bookmark_in_current_file(bookmark, window, cx);
                } else if let Some((path, bookmark)) = locations
                    .iter()
                    .find(|(path, bookmark)| {
                        path.as_ref() != current_path.as_ref() && bookmark.number == number
                    })
                    .cloned()
                {
                    self.open_numbered_bookmark_location(
                        &path,
                        bookmark.row,
                        bookmark.column,
                        window,
                        cx,
                    );
                } else {
                    self.warn_numbered_bookmark_not_defined(number, cx);
                }
            }
            NumberedBookmarkNavigateThroughAllFiles::AllowDuplicates => {
                if let Some((_, bookmark)) = current_file_location
                    && bookmark.row != current_row
                {
                    self.reveal_numbered_bookmark_in_current_file(bookmark, window, cx);
                    return;
                }

                let numbered: Vec<_> = locations
                    .into_iter()
                    .filter(|(_, bookmark)| bookmark.number == number)
                    .collect();
                if numbered.is_empty() {
                    self.warn_numbered_bookmark_not_defined(number, cx);
                    return;
                }

                let current_index = numbered
                    .iter()
                    .position(|(path, _)| path.as_ref() == current_path.as_ref());
                let next = match current_index {
                    Some(index) => numbered
                        .iter()
                        .skip(index + 1)
                        .chain(numbered.iter().take(index))
                        .find(|(path, _)| path.as_ref() != current_path.as_ref()),
                    None => numbered.first(),
                };

                if let Some((path, bookmark)) = next {
                    self.open_numbered_bookmark_location(
                        path,
                        bookmark.row,
                        bookmark.column,
                        window,
                        cx,
                    );
                } else {
                    self.warn_numbered_bookmark_not_defined(number, cx);
                }
            }
        }
    }

    fn reveal_numbered_bookmark_in_current_file(
        &mut self,
        bookmark: &SerializedNumberedBookmark,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot(window, cx);
        let multi_buffer_snapshot = snapshot.buffer_snapshot();
        let point =
            multi_buffer_snapshot.clip_point(Point::new(bookmark.row, bookmark.column), Bias::Left);
        let anchor = multi_buffer_snapshot.anchor_before(point);
        self.unfold_ranges(&[anchor..anchor], true, false, cx);
        self.change_selections(
            SelectionEffects::scroll(numbered_bookmark_autoscroll(cx)),
            window,
            cx,
            |selections| {
                selections.select_anchor_ranges([anchor..anchor]);
            },
        );
    }

    fn open_numbered_bookmark_location(
        &mut self,
        path: &Arc<Path>,
        row: u32,
        column: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace() else {
            return;
        };
        let path = path.to_path_buf();
        let task = workspace.update(cx, |workspace, cx| {
            workspace.open_abs_path(
                path,
                OpenOptions {
                    visible: Some(OpenVisible::All),
                    focus: Some(true),
                    ..Default::default()
                },
                window,
                cx,
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            let item = task.await?;
            this.update_in(cx, |_, window, cx| {
                if let Some(editor) = item.act_as::<Editor>(cx) {
                    editor.update(cx, |editor, cx| {
                        let snapshot = editor.snapshot(window, cx);
                        let point = snapshot
                            .buffer_snapshot()
                            .clip_point(Point::new(row, column), Bias::Left);
                        editor.change_selections(
                            SelectionEffects::scroll(numbered_bookmark_autoscroll(cx)),
                            window,
                            cx,
                            |selections| {
                                selections.select_ranges(std::iter::once(point..point));
                            },
                        );
                    });
                }
            })
        })
        .detach_and_log_err(cx);
    }

    fn warn_numbered_bookmark_not_defined(&self, number: u8, cx: &mut Context<Self>) {
        if !EditorSettings::get_global(cx)
            .numbered_bookmarks
            .show_not_defined_warning
        {
            return;
        }
        let Some(workspace) = self.workspace() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            workspace.show_toast(
                Toast::new(
                    NotificationId::unique::<NumberedBookmarkNotDefined>(),
                    format!("The Bookmark {number} is not defined"),
                )
                .autohide(),
                cx,
            );
        });
    }

    fn notify_no_numbered_bookmarks(&self, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            workspace.show_toast(
                Toast::new(
                    NotificationId::unique::<NoNumberedBookmarksFound>(),
                    "No Bookmarks found",
                )
                .autohide(),
                cx,
            );
        });
    }

    fn persist_numbered_bookmarks_project_file(&self, cx: &App) {
        if !EditorSettings::get_global(cx)
            .numbered_bookmarks
            .save_bookmarks_in_project
        {
            return;
        }
        let Some(bookmark_store) = self.bookmark_store.clone() else {
            return;
        };
        let Some(project) = self.project() else {
            return;
        };
        let fs = project.read(cx).fs().clone();
        bookmark_store
            .read(cx)
            .persist_vscode_project_files(fs, cx)
            .detach_and_log_err(cx);
    }

    fn show_numbered_bookmark_list(
        &mut self,
        all_files: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace() else {
            return;
        };
        let Some(bookmark_store) = self.bookmark_store.clone() else {
            return;
        };

        let current_path = self.buffer.read(cx).as_singleton().and_then(|buffer| {
            project::bookmark_store::BookmarkStore::abs_path_from_buffer(&buffer, cx)
        });

        if all_files {
            if !bookmark_store.read(cx).has_numbered_bookmarks() {
                self.notify_no_numbered_bookmarks(cx);
                return;
            }
        } else if let Some(buffer) = self.buffer.read(cx).as_singleton() {
            if !bookmark_store.update(cx, |store, cx| {
                store.has_numbered_bookmarks_in_buffer(&buffer, cx)
            }) {
                self.notify_no_numbered_bookmarks(cx);
                return;
            }
        } else {
            self.notify_no_numbered_bookmarks(cx);
            return;
        }

        let locations = bookmark_store.read(cx).numbered_bookmark_locations(cx);
        let mut items = Vec::new();
        for (path, bookmark) in locations {
            let in_active_file = current_path
                .as_ref()
                .is_some_and(|current| current.as_ref() == path.as_ref());
            if !all_files && !in_active_file {
                continue;
            }
            let line_text = bookmark_store.read(cx).line_text(&path, bookmark.row, cx);
            items.push(NumberedBookmarkListItem {
                path,
                number: bookmark.number,
                row: bookmark.row,
                column: bookmark.column,
                line_text,
                in_active_file,
            });
        }

        if items.is_empty() {
            self.notify_no_numbered_bookmarks(cx);
            return;
        }

        let snapshot = self.snapshot(window, cx);
        let previous_selection = self
            .selections
            .newest::<Point>(&snapshot.display_snapshot)
            .range();
        let editor_handle = cx.entity();

        workspace.update(cx, |workspace, cx| {
            workspace.toggle_modal(window, cx, |window, cx| {
                NumberedBookmarkList::new(
                    editor_handle,
                    items,
                    previous_selection,
                    all_files,
                    window,
                    cx,
                )
            });
        });
    }

    pub(crate) fn active_numbered_bookmarks(
        &self,
        range: Range<DisplayRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> HashMap<DisplayRow, u8> {
        let mut numbered = HashMap::default();
        let Some(bookmark_store) = self.bookmark_store.clone() else {
            return numbered;
        };
        let Some(project) = self.project() else {
            return numbered;
        };

        let snapshot = self.snapshot(window, cx);
        let multi_buffer_snapshot = snapshot.buffer_snapshot();
        let range = snapshot
            .display_point_to_point(crate::DisplayPoint::new(range.start, 0), Bias::Left)
            ..snapshot.display_point_to_point(crate::DisplayPoint::new(range.end, 0), Bias::Right);

        for (buffer_snapshot, buffer_range, _) in
            multi_buffer_snapshot.range_to_buffer_ranges(range.start..range.end)
        {
            let Some(buffer) = project
                .read(cx)
                .buffer_for_id(buffer_snapshot.remote_id(), cx)
            else {
                continue;
            };
            let bookmarks = bookmark_store.update(cx, |store, cx| {
                store.numbered_bookmarks_for_buffer(
                    buffer,
                    buffer_snapshot.anchor_before(buffer_range.start)
                        ..buffer_snapshot.anchor_after(buffer_range.end),
                    &buffer_snapshot,
                    cx,
                )
            });
            for bookmark in bookmarks {
                let Some(multi_buffer_anchor) =
                    multi_buffer_snapshot.anchor_in_buffer(bookmark.anchor)
                else {
                    continue;
                };
                let display_row = multi_buffer_anchor
                    .to_point(&multi_buffer_snapshot)
                    .to_display_point(&snapshot)
                    .row();
                numbered.insert(display_row, bookmark.number);
            }
        }

        numbered
    }

    pub(crate) fn render_numbered_bookmark(
        &self,
        row: DisplayRow,
        number: u8,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = EditorSettings::get_global(cx);
        let fill = parse_color(&settings.numbered_bookmarks.gutter_icon_fill_color)
            .unwrap_or_else(|| gpui::hsla(0.33, 1.0, 0.5, 1.0));
        let number_color = parse_color(&settings.numbered_bookmarks.gutter_icon_number_color)
            .unwrap_or(gpui::black());

        div()
            .id(("numbered bookmark", row.0 as usize))
            .relative()
            .size(px(14.))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .cursor_pointer()
            .child(
                Icon::new(IconName::Bookmark)
                    .size(IconSize::Small)
                    .color(Color::Custom(fill)),
            )
            .child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .pb(px(2.))
                    .text_color(number_color)
                    .text_size(px(8.))
                    .font_weight(FontWeight::BOLD)
                    .child(number.to_string()),
            )
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(cx.listener(move |editor, _, _, cx| {
                cx.stop_propagation();
                editor.toggle_numbered_bookmark_at_row(number, row, cx);
            }))
            .into_any_element()
    }

    pub(crate) fn refresh_numbered_bookmark_highlights(&mut self, cx: &mut Context<Self>) {
        self.clear_row_highlights::<NumberedBookmarkRowHighlights>();
        let Some(bookmark_store) = self.bookmark_store.clone() else {
            return;
        };
        let Some(project) = self.project().cloned() else {
            return;
        };
        let has_line_background = parse_color(
            &EditorSettings::get_global(cx)
                .numbered_bookmarks
                .line_background,
        )
        .is_some_and(|color| color.a > 0.0);
        if !has_line_background {
            cx.notify();
            return;
        }

        let snapshot = self.buffer.read(cx).snapshot(cx);
        let mut ranges = Vec::new();
        for (buffer_snapshot, buffer_range, _) in
            snapshot.range_to_buffer_ranges(Point::zero()..snapshot.max_point())
        {
            let Some(buffer) = project
                .read(cx)
                .buffer_for_id(buffer_snapshot.remote_id(), cx)
            else {
                continue;
            };
            let bookmarks = bookmark_store.update(cx, |store, cx| {
                store.numbered_bookmarks_for_buffer(
                    buffer,
                    buffer_snapshot.anchor_before(buffer_range.start)
                        ..buffer_snapshot.anchor_after(buffer_range.end),
                    &buffer_snapshot,
                    cx,
                )
            });
            for bookmark in bookmarks {
                let Some(start) = snapshot.anchor_in_buffer(bookmark.anchor) else {
                    continue;
                };
                let row = start.to_point(&snapshot).row;
                let start = snapshot.anchor_before(Point::new(row, 0));
                let max_point = snapshot.max_point();
                let end = if row >= max_point.row {
                    snapshot.anchor_after(max_point)
                } else {
                    snapshot.anchor_before(Point::new(row + 1, 0))
                };
                ranges.push(start..end);
            }
        }
        for range in ranges {
            self.highlight_rows::<NumberedBookmarkRowHighlights>(
                range,
                numbered_bookmark_line_background,
                crate::RowHighlightOptions::default(),
                cx,
            );
        }
        cx.notify();
    }
}

impl NumberedBookmarkList {
    fn new(
        active_editor: Entity<Editor>,
        items: Vec<NumberedBookmarkListItem>,
        previous_selection: Range<Point>,
        all_files: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let picker_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(
                "Type a line number or a piece of code to navigate to",
                window,
                cx,
            );
            editor
        });
        cx.subscribe(&picker_editor, |this, _, event: &crate::EditorEvent, cx| {
            if matches!(event, crate::EditorEvent::BufferEdited) {
                this.update_matches(cx);
            }
        })
        .detach();

        let matches: Vec<usize> = (0..items.len()).collect();
        let this = Self {
            picker_editor,
            active_editor,
            items,
            matches,
            selected_index: 0,
            previous_selection,
            confirmed: false,
            all_files,
        };
        this.preview_selected(window, cx);
        this
    }

    fn query(&self, cx: &App) -> String {
        self.picker_editor.read(cx).text(cx).to_lowercase()
    }

    fn update_matches(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        self.matches = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if query.is_empty() {
                    return true;
                }
                let description = format!(
                    "(Ln {}, Col {}) {}",
                    item.row + 1,
                    item.column + 1,
                    item.path.display()
                );
                item.line_text.to_lowercase().contains(&query)
                    || description.to_lowercase().contains(&query)
                    || item.number.to_string() == query
            })
            .map(|(index, _)| index)
            .collect();
        self.selected_index = self
            .selected_index
            .min(self.matches.len().saturating_sub(1));
        cx.notify();
    }

    fn selected_item(&self) -> Option<&NumberedBookmarkListItem> {
        self.matches
            .get(self.selected_index)
            .and_then(|index| self.items.get(*index))
    }

    fn preview_selected(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item().cloned() else {
            return;
        };
        if item.in_active_file {
            self.active_editor.update(cx, |editor, cx| {
                editor.reveal_numbered_bookmark_in_current_file(
                    &SerializedNumberedBookmark {
                        number: item.number,
                        row: item.row,
                        column: item.column,
                    },
                    window,
                    cx,
                );
            });
        }
    }

    fn select_next(&mut self, _: &menu::SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.matches.len();
        self.preview_selected(window, cx);
        cx.notify();
    }

    fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.matches.is_empty() {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.matches.len() - 1;
        } else {
            self.selected_index -= 1;
        }
        self.preview_selected(window, cx);
        cx.notify();
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item().cloned() else {
            return;
        };
        self.confirmed = true;
        self.active_editor.update(cx, |editor, cx| {
            if item.in_active_file {
                editor.reveal_numbered_bookmark_in_current_file(
                    &SerializedNumberedBookmark {
                        number: item.number,
                        row: item.row,
                        column: item.column,
                    },
                    window,
                    cx,
                );
            } else {
                editor.open_numbered_bookmark_location(
                    &item.path,
                    item.row,
                    item.column,
                    window,
                    cx,
                );
            }
        });
        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, _: &menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl Render for NumberedBookmarkList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_index = self.selected_index;
        v_flex()
            .w(rems(34.))
            .max_h(rems(24.))
            .elevation_2(cx)
            .key_context("Picker")
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .child(
                div()
                    .border_b_1()
                    .border_color(cx.theme().colors().border_variant)
                    .px_2()
                    .py_1()
                    .child(self.picker_editor.clone()),
            )
            .child(
                v_flex().p_1().children(self.matches.iter().enumerate().map(
                    |(match_index, item_index)| {
                        let item = &self.items[*item_index];
                        let description = if item.in_active_file || !self.all_files {
                            format!("(Ln {}, Col {})", item.row + 1, item.column + 1)
                        } else {
                            format!(
                                "{}  (Ln {}, Col {})",
                                item.path.display(),
                                item.row + 1,
                                item.column + 1
                            )
                        };
                        ListItem::new(("numbered-bookmark-item", *item_index))
                            .spacing(ListItemSpacing::Sparse)
                            .toggle_state(match_index == selected_index)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.selected_index = match_index;
                                this.confirm(&menu::Confirm, window, cx);
                            }))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Label::new(item.number.to_string())
                                            .size(LabelSize::Small)
                                            .color(Color::Accent),
                                    )
                                    .child(Label::new(item.line_text.clone()))
                                    .child(
                                        Label::new(description)
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    ),
                            )
                    },
                )),
            )
    }
}

fn numbered_bookmark_autoscroll(cx: &App) -> Autoscroll {
    match EditorSettings::get_global(cx)
        .numbered_bookmarks
        .reveal_location
    {
        NumberedBookmarkRevealLocation::Top => Autoscroll::top(),
        NumberedBookmarkRevealLocation::Center => Autoscroll::center(),
    }
}

fn numbered_bookmark_line_background(cx: &App) -> Hsla {
    parse_color(
        &EditorSettings::get_global(cx)
            .numbered_bookmarks
            .line_background,
    )
    .unwrap_or(gpui::transparent_black())
}

fn parse_color(value: &str) -> Option<Hsla> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Rgba::try_from(trimmed).ok().map(Hsla::from)
}
