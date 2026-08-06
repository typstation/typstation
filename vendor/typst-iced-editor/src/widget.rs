//! The code editor widget.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::OnceLock;

use iced_core::clipboard::{self, Clipboard};
use iced_core::input_method::{self, InputMethod};
use iced_core::keyboard;
use iced_core::layout::{self, Layout};
use iced_core::overlay;
use iced_core::renderer::{self, Quad};
use iced_core::svg;
use iced_core::text::{self, Paragraph as _, Text};
use iced_core::time::{Duration, Instant};
use iced_core::widget::{operation, tree, Operation, Tree, Widget};
use iced_core::{alignment, mouse, window};
use iced_core::{
    Element, Event, Font, Length, Padding, Pixels, Point, Rectangle, Shell, Size, Vector,
};

use crate::action::Action;
use crate::complete::Completion;
use crate::content::{Content, Internal};
use crate::cursor::Motion;
use crate::diagnostic::Severity;
use crate::draw::{
    draw_fold_guide, draw_preedit, draw_range_highlights, draw_squiggle, for_each_range_segment,
    next_grapheme_end, visible_lines, Frame,
};
use crate::fold::Fold;
use crate::keymap::{Binding, KeyBindingFn, KeyPress};
use crate::line_cache::{
    caret_geometry, hover_offset_at, offset_at, offset_at_row, reveal_caret, LineCache,
};
use crate::overlay::{CompletionPopup, PopupMetrics, Tooltip};
use crate::pair::matching_delimiter_ranges;
use crate::scroll::{
    begin_scrollbar_drag, draw_scrollbar, scrollbar_geometries, scrollbar_target, scrollbar_ticks,
    Axis, ScrollbarDrag, ScrollbarGeometry, SmoothScroll,
};
use crate::style::{Catalog, Status, Style, StyleFn};

/// Creates a new [`CodeEditor`] for the given [`Content`].
///
/// The fold markers and diagnostic squiggles are embedded SVG assets, so
/// turning the editor into an [`Element`] requires a renderer that also
/// implements [`svg::Renderer`] — with the standard iced renderer, enable
/// iced's `svg` feature (`iced = { version = "0.14", features = ["svg"] }`).
/// Without it, the build fails with an `E0277` error about
/// `iced_wgpu::Renderer` that never mentions this crate or the feature.
pub fn code_editor<'a, Message, Theme, Renderer>(
    content: &'a Content,
) -> CodeEditor<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font>,
{
    CodeEditor::new(content)
}

/// When to draw the vertical guide that shows a foldable block's extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FoldGuides {
    /// Never draw guides.
    None,
    /// Only the innermost block containing the caret, emphasized.
    #[default]
    Current,
    /// Every foldable block, faintly, with the caret's block emphasized.
    All,
}

/// A multi-line editor for Typst code, with syntax highlighting, soft wrap,
/// a line number gutter, and full mouse and keyboard editing.
///
/// The widget is stateless in the Elm sense: it borrows a [`Content`] and
/// publishes [`Action`]s through [`on_action`](Self::on_action); the
/// application applies them with [`Content::perform`].
///
/// The fold markers and diagnostic squiggles are embedded SVG assets, so
/// the `Renderer` must implement [`svg::Renderer`] in addition to
/// [`text::Renderer`]; with the standard iced renderer this means enabling
/// iced's `svg` feature.
#[allow(missing_debug_implementations)]
pub struct CodeEditor<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font>,
{
    content: &'a Content,
    on_action: Option<Box<dyn Fn(Action) -> Message + 'a>>,
    on_context_menu: Option<Box<dyn Fn(Point, usize) -> Message + 'a>>,
    key_binding: Option<KeyBindingFn<'a, Message>>,
    completion_enabled: bool,
    completion_triggers: Vec<char>,
    hover_enabled: bool,
    width: Length,
    height: Length,
    padding: Padding,
    font: Option<Font>,
    text_size: Option<Pixels>,
    line_height: text::LineHeight,
    wrap: bool,
    indent_wrapped: bool,
    gutter: bool,
    fold_guides: FoldGuides,
    scrollbar: bool,
    hover_delay: Duration,
    class: Theme::Class<'a>,
    _renderer: PhantomData<Renderer>,
}

impl<'a, Message, Theme, Renderer> CodeEditor<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font>,
{
    /// Creates a new [`CodeEditor`] for the given [`Content`].
    pub fn new(content: &'a Content) -> Self {
        Self {
            content,
            on_action: None,
            on_context_menu: None,
            key_binding: None,
            completion_enabled: false,
            completion_triggers: vec!['#', '@'],
            hover_enabled: false,
            width: Length::Fill,
            height: Length::Fill,
            padding: Padding::new(5.0),
            font: None,
            text_size: None,
            line_height: text::LineHeight::default(),
            wrap: true,
            indent_wrapped: true,
            gutter: true,
            fold_guides: FoldGuides::default(),
            scrollbar: true,
            hover_delay: HoverPhase::DEFAULT_DELAY,
            class: <Theme as Catalog>::default(),
            _renderer: PhantomData,
        }
    }

    /// Sets the message to produce when an [`Action`] is performed.
    ///
    /// Without this, the editor is disabled: it renders but cannot be
    /// interacted with.
    pub fn on_action(mut self, on_action: impl Fn(Action) -> Message + 'a) -> Self {
        self.on_action = Some(Box::new(on_action));
        self
    }

    /// Sets the message produced by a right click over the editor.
    ///
    /// The callback receives the pointer position in window coordinates and
    /// the byte offset under it, using the editor's own text hit-testing.
    pub fn on_context_menu(
        mut self,
        on_context_menu: impl Fn(Point, usize) -> Message + 'a,
    ) -> Self {
        self.on_context_menu = Some(Box::new(on_context_menu));
        self
    }

    /// Overrides the key bindings of the editor.
    ///
    /// The function receives every [`KeyPress`] and decides its [`Binding`];
    /// call [`Binding::from_key_press`] as the fallback to keep the default
    /// behavior for keys you do not handle.
    pub fn key_binding(
        mut self,
        key_binding: impl Fn(KeyPress) -> Option<Binding<Message>> + 'a,
    ) -> Self {
        self.key_binding = Some(Box::new(key_binding));
        self
    }

    /// Enables the completion popup.
    ///
    /// The popup opens on Ctrl+Space (see [`Binding::Complete`]) and
    /// automatically after `#` or `@` is typed — the starts of a Typst call
    /// and reference, so the flow feels native. Change or disable those
    /// characters with [`completion_triggers`](Self::completion_triggers).
    ///
    /// In every case the editor only emits
    /// [`Action::RequestCompletions`](crate::Action::RequestCompletions); the
    /// application provides the items asynchronously with
    /// [`Content::set_completions`](crate::Content::set_completions).
    pub fn completions(mut self) -> Self {
        self.completion_enabled = true;
        self
    }

    /// Sets the characters that open the completion popup automatically as
    /// they are typed, replacing the default `['#', '@']`. Pass an empty
    /// iterator to open only on Ctrl+Space. Also enables completions.
    ///
    /// The application still provides the items by answering
    /// [`Action::RequestCompletions`](crate::Action::RequestCompletions).
    pub fn completion_triggers(mut self, triggers: impl IntoIterator<Item = char>) -> Self {
        self.completion_enabled = true;
        self.completion_triggers = triggers.into_iter().collect();
        self
    }

    /// Enables hover tooltips from a provider.
    ///
    /// When the pointer rests over the text, the editor emits
    /// [`Action::RequestHover`](crate::Action::RequestHover) and shows the
    /// tooltip delivered with
    /// [`Content::set_hover`](crate::Content::set_hover). Diagnostics show
    /// their message on hover regardless of this setting.
    pub fn hover(mut self) -> Self {
        self.hover_enabled = true;
        self
    }

    /// Sets how long the pointer must rest before a hover tooltip appears.
    /// Defaults to 500ms.
    pub fn hover_delay(mut self, delay: Duration) -> Self {
        self.hover_delay = delay;
        self
    }

    /// Sets the width of the editor.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the editor.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the [`Padding`] of the editor.
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets the font of the editor. Defaults to [`Font::MONOSPACE`].
    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Sets the text size of the editor.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.text_size = Some(size.into());
        self
    }

    /// Sets the line height of the editor.
    pub fn line_height(mut self, line_height: impl Into<text::LineHeight>) -> Self {
        self.line_height = line_height.into();
        self
    }

    /// Enables or disables soft wrap. Enabled by default.
    ///
    /// With soft wrap, long lines flow into multiple visual rows and there
    /// is no horizontal scrolling.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Aligns the wrapped rows of a line under its indentation, so a soft-
    /// wrapped indented line reads as one block. Purely visual — the text is
    /// unchanged. Enabled by default; only affects wrapped lines.
    pub fn indent_wrapped_lines(mut self, indent_wrapped: bool) -> Self {
        self.indent_wrapped = indent_wrapped;
        self
    }

    /// Shows or hides the line number gutter. Shown by default.
    pub fn gutter(mut self, gutter: bool) -> Self {
        self.gutter = gutter;
        self
    }

    /// Sets when to draw the vertical guide showing a foldable block's extent.
    /// Defaults to [`FoldGuides::Current`] (only the block the caret is in).
    /// Guides are drawn in the text area, aligned with the block indentation.
    pub fn fold_guides(mut self, fold_guides: FoldGuides) -> Self {
        self.fold_guides = fold_guides;
        self
    }

    /// Shows or hides the editor-owned vertical scrollbar. Shown by default.
    ///
    /// The scrollbar is drawn as an overlay and does not participate in
    /// layout, so it never changes soft-wrap measurements.
    pub fn scrollbar(mut self, scrollbar: bool) -> Self {
        self.scrollbar = scrollbar;
        self
    }

    /// Sets the style of the editor.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the style class of the editor.
    #[must_use]
    pub fn class(mut self, class: impl Into<Theme::Class<'a>>) -> Self {
        self.class = class.into();
        self
    }

    /// Computes the geometry shared by `update`, `draw`, and
    /// `mouse_interaction`.
    fn metrics(&self, renderer: &Renderer, bounds: Rectangle, line_count: usize) -> Metrics {
        let font = self.font.unwrap_or(Font::MONOSPACE);
        let size = self.text_size.unwrap_or_else(|| renderer.default_size());
        let line_height = self.line_height.to_absolute(size).0;

        let inner = bounds.shrink(self.padding);

        // Measure one digit to size the gutter; with a monospaced font this
        // is exact, and a good approximation otherwise.
        let digit_width = Renderer::Paragraph::with_text(Text {
            content: "0",
            bounds: Size::INFINITE,
            size,
            line_height: text::LineHeight::Absolute(Pixels(line_height)),
            font,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
        })
        .min_bounds()
        .width;

        let digits = (line_count.max(1).ilog10() as usize + 1).max(2);

        let (gutter, text_area) = if self.gutter {
            // Space for the fold marker, line numbers, and comfortable padding.
            let width = (digits as f32 + 5.0) * digit_width;
            let (gutter, text_area) = split_horizontally(inner, width);

            (Some(gutter), text_area)
        } else {
            (None, inner)
        };

        Metrics {
            font,
            size: size.0,
            line_height,
            digit_width,
            digits,
            gutter,
            text_area,
        }
    }

    /// Synchronizes the line cache and the shared view geometry with the
    /// current content. Returns with the wrap map up to date.
    fn sync(
        &self,
        internal: &mut Internal,
        cache: &mut LineCache<Renderer::Paragraph>,
        metrics: &Metrics,
    ) {
        cache.sync(internal, metrics, self.wrap, self.indent_wrapped);

        internal.view.line_height = metrics.line_height;
        internal.view.size = metrics.text_area.size();
        internal.view.total_rows = cache.total_rows() as usize;

        if self.wrap {
            internal.scroll.x = 0.0;
            internal.view.max_line_width = 0.0;
            internal.view.revision = internal.buffer.revision();
        } else if internal.view.revision != internal.buffer.revision() {
            internal.view.revision = internal.buffer.revision();
            internal.view.max_line_width = 0.0;
        }
    }

    /// Borrows the document, resolves the frame [`Metrics`], synchronizes the
    /// [`LineCache`], and hands them to `f` along with the displayed scroll —
    /// the preamble shared by every input handler that hit-tests or measures.
    fn with_layout<R>(
        &self,
        state: &State<Renderer::Paragraph>,
        renderer: &Renderer,
        bounds: Rectangle,
        f: impl FnOnce(&mut Internal, &Metrics, &mut LineCache<Renderer::Paragraph>, Vector) -> R,
    ) -> R {
        let mut internal = self.content.0.borrow_mut();
        let internal = &mut *internal;
        let metrics = self.metrics(renderer, bounds, internal.buffer.line_count());
        let mut cache = state.cache.borrow_mut();
        self.sync(internal, &mut cache, &metrics);
        let scroll = state.smooth_scroll.borrow_mut().current(internal.scroll);

        f(internal, &metrics, &mut cache, scroll)
    }
}

/// The resolved geometry of the editor for one frame.
pub(crate) struct Metrics {
    pub font: Font,
    pub size: f32,
    pub line_height: f32,
    pub digit_width: f32,
    pub digits: usize,
    pub gutter: Option<Rectangle>,
    pub text_area: Rectangle,
}

impl Metrics {
    /// The number of whole rows that fit in the viewport.
    pub fn rows_in_view(&self) -> u64 {
        (self.text_area.height / self.line_height).floor().max(1.0) as u64
    }

    /// The pixel height of the whole document, given its total visual rows.
    pub fn content_height(&self, total_rows: u64) -> f32 {
        (total_rows.max(1) as f32 * self.line_height).max(self.text_area.height)
    }

    /// The gutter's inner layout, if the gutter is shown. Drawing and
    /// hit-testing must agree on it, so it is computed in one place.
    pub fn gutter_layout(&self) -> Option<GutterLayout> {
        let gutter = self.gutter?;
        let number_right = gutter.x + (self.digits as f32 + 1.0) * self.digit_width;

        Some(GutterLayout {
            number_right,
            marker: Rectangle {
                x: number_right + self.digit_width,
                y: gutter.y,
                width: self.digit_width * 3.0,
                height: gutter.height,
            },
        })
    }
}

/// The x positions inside the gutter: line numbers right-align at
/// `number_right` and the fold marker is centered in `marker`.
pub(crate) struct GutterLayout {
    pub number_right: f32,
    pub marker: Rectangle,
}

fn split_horizontally(bounds: Rectangle, width: f32) -> (Rectangle, Rectangle) {
    let width = width.min(bounds.width);

    (
        Rectangle { width, ..bounds },
        Rectangle {
            x: bounds.x + width,
            width: bounds.width - width,
            ..bounds
        },
    )
}

/// The resolved highlight inputs shared by the text-area and scrollbar
/// passes.
struct Marks {
    search_matches: Vec<Range<usize>>,
    current_search_match: Option<usize>,
    delimiter_matches: Vec<Range<usize>>,
    diagnostics: Vec<(Range<usize>, Severity)>,
}

/// The internal state of the widget, kept by the runtime across frames.
struct State<P: text::Paragraph> {
    focus: Option<Focus>,
    modifiers: keyboard::Modifiers,
    last_click: Option<mouse::Click>,
    drag_click: Option<mouse::click::Kind>,
    preedit: Option<input_method::Preedit>,
    /// The pixel column the caret aims for during vertical movement.
    goal_x: Option<f32>,
    /// The open completion popup's UI state, if any.
    completion: Option<CompletionUi>,
    /// The state of the hover tooltip.
    hover: HoverPhase,
    /// The next id to stamp on an intelligence request.
    next_request_id: u64,
    /// The scroll offset used for drawing, interpolated toward the real
    /// content scroll.
    smooth_scroll: RefCell<SmoothScroll>,
    scrollbar_drag: Option<ScrollbarDrag>,
    cache: RefCell<LineCache<P>>,
}

/// The transient UI state of an open completion popup.
///
/// The candidate items themselves live in the [`Content`] (delivered
/// asynchronously); this only holds the selection and scroll, plus the
/// bookkeeping to reconcile requests with responses.
pub(crate) struct CompletionUi {
    /// The selected item index.
    pub selected: usize,
    /// The first visible item, so long lists scroll.
    pub scroll: usize,
    /// The lowest request id whose results this popup will display. Results
    /// from before the popup opened are ignored, so reopening never flashes
    /// stale candidates.
    session_min_id: u64,
    /// A fresh request must be emitted on the next redraw (open or refine).
    needs_request: bool,
    /// The character and byte offset that opened an automatic session.
    /// Explicit sessions opened with Ctrl+Space have no trigger.
    trigger: Option<(usize, char)>,
}

impl CompletionUi {
    /// The number of items shown at once before the list scrolls.
    pub const VISIBLE: usize = 10;

    fn select(&mut self, index: usize, count: usize) {
        self.selected = index.min(count.saturating_sub(1));

        // Keep the selection within the visible window.
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + Self::VISIBLE {
            self.scroll = self.selected + 1 - Self::VISIBLE;
        }
    }

    fn select_next(&mut self, count: usize) {
        let next = if count == 0 {
            0
        } else {
            (self.selected + 1) % count
        };
        self.select(next, count);
    }

    fn select_previous(&mut self, count: usize) {
        let previous = if self.selected == 0 {
            count.saturating_sub(1)
        } else {
            self.selected - 1
        };
        self.select(previous, count);
    }
}

/// The lifecycle of the hover tooltip.
enum HoverPhase {
    /// The pointer is not resting anywhere relevant.
    Idle,
    /// The pointer came to rest; the tooltip appears once the delay passes.
    Pending { at: Point, since: Instant },
    /// A hover request has been emitted and is awaiting a response.
    Requested { at: Point, id: u64 },
    /// The tooltip is shown at `at` with the given content.
    Shown { at: Point, content: String },
}

impl HoverPhase {
    /// The default delay before a tooltip appears.
    const DEFAULT_DELAY: Duration = Duration::from_millis(500);
}

struct Focus {
    updated_at: Instant,
    now: Instant,
    is_window_focused: bool,
}

impl Focus {
    const CURSOR_BLINK_INTERVAL_MILLIS: u128 = 500;

    fn now() -> Self {
        let now = Instant::now();

        Self {
            updated_at: now,
            now,
            is_window_focused: true,
        }
    }

    fn is_cursor_visible(&self) -> bool {
        self.is_window_focused
            && ((self.now - self.updated_at).as_millis() / Self::CURSOR_BLINK_INTERVAL_MILLIS)
                .is_multiple_of(2)
    }
}

const FOLD_EXPANDED: &[u8] = include_bytes!("../assets/chevron_down.svg");
const FOLD_COLLAPSED: &[u8] = include_bytes!("../assets/chevron_right.svg");

static FOLD_EXPANDED_HANDLE: OnceLock<svg::Handle> = OnceLock::new();
static FOLD_COLLAPSED_HANDLE: OnceLock<svg::Handle> = OnceLock::new();

impl<P: text::Paragraph + 'static> operation::Focusable for State<P> {
    fn is_focused(&self) -> bool {
        self.focus.is_some()
    }

    fn focus(&mut self) {
        self.focus = Some(Focus::now());
    }

    fn unfocus(&mut self) {
        self.focus = None;
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for CodeEditor<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font> + svg::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer::Paragraph> {
            focus: None,
            modifiers: keyboard::Modifiers::default(),
            last_click: None,
            drag_click: None,
            preedit: None,
            goal_x: None,
            completion: None,
            hover: HoverPhase::Idle,
            next_request_id: 0,
            smooth_scroll: RefCell::new(SmoothScroll::new()),
            scrollbar_drag: None,
            cache: RefCell::new(LineCache::new()),
        })
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();

        operation.focusable(None, layout.bounds(), state);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if self.on_action.is_none() {
            return;
        };

        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let bounds = layout.bounds();

        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Window(window::Event::Unfocused) => {
                if let Some(focus) = &mut state.focus {
                    focus.is_window_focused = false;
                }
            }
            Event::Window(window::Event::Focused) => {
                if let Some(focus) = &mut state.focus {
                    focus.is_window_focused = true;
                    focus.updated_at = Instant::now();

                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if let Some(focus) = &mut state.focus {
                    if focus.is_window_focused {
                        focus.now = *now;

                        let millis_until_blink = Focus::CURSOR_BLINK_INTERVAL_MILLIS
                            - (focus.now - focus.updated_at).as_millis()
                                % Focus::CURSOR_BLINK_INTERVAL_MILLIS;

                        shell.request_redraw_at(
                            focus.now + Duration::from_millis(millis_until_blink as u64),
                        );
                    }
                }

                let mut input_method_caret = None;
                let (scroll_is_moving, overscan_pending, fold_walk_due) = {
                    let mut internal = self.content.0.borrow_mut();
                    let internal = &mut *internal;
                    let metrics = self.metrics(renderer, bounds, internal.buffer.line_count());
                    let mut cache = state.cache.borrow_mut();

                    // A fold just toggled: remember the buffer line at the top
                    // of the viewport, measured against the *pre-toggle* row
                    // map still in the cache, so it can be pinned there once
                    // the relayout below removes or restores the folded rows.
                    let fold_anchor = internal.fold_anchor.then(|| {
                        let top_row = (internal.scroll.y / metrics.line_height).max(0.0);
                        let top_line = cache.line_at_row(top_row.floor() as u64);
                        let rows_into_line = top_row - cache.first_row(top_line) as f32;
                        (top_line, rows_into_line.max(0.0))
                    });

                    cache.allow_overscan(*now);
                    self.sync(internal, &mut cache, &metrics);

                    if let Some((top_line, rows_into_line)) = fold_anchor {
                        internal.fold_anchor = false;
                        let top_row = cache.first_row(top_line) as f32 + rows_into_line;
                        internal.scroll.y = top_row * metrics.line_height;
                        internal.clamp_scroll();
                        // Snap: the content under the fold shifts instantly, it
                        // does not slide.
                        state.smooth_scroll.borrow_mut().jump_to(internal.scroll);
                    }

                    if internal.needs_reveal {
                        internal.needs_reveal = false;
                        reveal_caret(internal, &metrics, &mut cache, self.wrap);
                    }

                    internal.clamp_scroll();

                    let (scroll, moving) = state
                        .smooth_scroll
                        .borrow_mut()
                        .advance(internal.scroll, *now);

                    if state.focus.is_some() {
                        let (line, row, x) = caret_geometry(internal, &mut cache);
                        let row_top =
                            (cache.first_row(line) + u64::from(row)) as f32 * metrics.line_height;

                        input_method_caret = Some(Rectangle {
                            x: metrics.text_area.x + x - scroll.x,
                            y: metrics.text_area.y + row_top - scroll.y,
                            width: 2.0,
                            height: metrics.line_height,
                        });
                    }

                    (moving, cache.overscan_pending(), cache.fold_walk_due())
                };

                if let Some(caret) = input_method_caret {
                    // The preedit is rendered in place by `draw`, so the
                    // runtime does not need to overlay it.
                    shell.request_input_method::<String>(&InputMethod::Enabled {
                        cursor: caret,
                        purpose: input_method::Purpose::Normal,
                        preedit: None,
                    });
                }

                if scroll_is_moving || overscan_pending {
                    shell.request_redraw();
                }

                // Deferred fold discovery: make sure a frame arrives once the
                // debounce elapses, even with nothing else animating.
                if let Some(due) = fold_walk_due {
                    shell.request_redraw_at(due);
                }

                self.emit_completion_request(state, shell);

                if self.resolve_hover(state, renderer, bounds, *now, shell) {
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                self.on_mouse_press(state, renderer, bounds, cursor, shell);
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                self.open_context_menu(state, renderer, bounds, cursor, shell);
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.drag_click = None;
                state.scrollbar_drag = None;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(drag) = state.scrollbar_drag {
                    let Some(position) = cursor.position() else {
                        return;
                    };

                    // Mouse moves arrive far more often than frames, so this
                    // path stays trivial: the geometry is frozen at press time,
                    // no cache sync, no line measurement.
                    let target = scrollbar_target(&drag.geometry, position, drag.grab);
                    let current = self.content.0.borrow().scroll;
                    let (action, scroll) = scroll_to(drag.geometry.axis, target, current);

                    state.smooth_scroll.borrow_mut().jump_to(scroll);
                    shell.publish((self.on_action.as_ref().unwrap())(action));
                    shell.capture_event();
                    shell.request_redraw();

                    return;
                }

                if state.drag_click == Some(mouse::click::Kind::Single) {
                    let Some(position) = cursor.position() else {
                        return;
                    };

                    let offset = self.with_layout(
                        state,
                        renderer,
                        bounds,
                        |internal, metrics, cache, scroll| {
                            offset_at(position, metrics, internal, cache, scroll)
                        },
                    );

                    shell.publish((self.on_action.as_ref().unwrap())(Action::SelectTo(offset)));

                    return;
                }

                // Restart the hover timer whenever the pointer moves.
                let has_hover =
                    self.hover_enabled || !self.content.0.borrow().diagnostics.is_empty();

                if has_hover {
                    match cursor.position_over(bounds) {
                        Some(position) => {
                            state.hover = HoverPhase::Pending {
                                at: position,
                                since: Instant::now(),
                            };
                            shell.request_redraw_at(Instant::now() + self.hover_delay);
                        }
                        None => {
                            if !matches!(state.hover, HoverPhase::Idle) {
                                state.hover = HoverPhase::Idle;
                                shell.request_redraw();
                            }
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let line_height = self
                    .line_height
                    .to_absolute(self.text_size.unwrap_or_else(|| renderer.default_size()))
                    .0;

                let (mut x, mut y) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => {
                        (-x * 3.0 * line_height, -y * 3.0 * line_height)
                    }
                    mouse::ScrollDelta::Pixels { x, y } => (-x, -y),
                };

                if state.modifiers.shift() && x == 0.0 {
                    std::mem::swap(&mut x, &mut y);
                }

                // The pointer holds still while the wheel turns, so no
                // CursorMoved arrives to refresh the tooltip — yet the text
                // scrolls out from under it. Dismiss it; it reappears once the
                // pointer next rests.
                if !matches!(state.hover, HoverPhase::Idle) {
                    state.hover = HoverPhase::Idle;
                }

                shell.publish((self.on_action.as_ref().unwrap())(Action::Scroll { x, y }));
                shell.capture_event();
                shell.request_redraw();
            }
            Event::InputMethod(input_method::Event::Opened) => {
                state.preedit = Some(input_method::Preedit::new());
                shell.request_redraw();
            }
            Event::InputMethod(input_method::Event::Closed) => {
                state.preedit = None;
                shell.request_redraw();
            }
            Event::InputMethod(input_method::Event::Preedit(content, selection))
                if state.focus.is_some() =>
            {
                state.preedit = Some(input_method::Preedit {
                    content: content.clone(),
                    selection: selection.clone(),
                    text_size: self.text_size,
                });

                shell.request_redraw();
            }
            Event::InputMethod(input_method::Event::Commit(text)) if state.focus.is_some() => {
                state.goal_x = None;
                shell.publish((self.on_action.as_ref().unwrap())(Action::Insert(
                    text.clone(),
                )));
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            }) => {
                if state.focus.is_none() {
                    return;
                }

                let press = KeyPress {
                    key: key.clone(),
                    modified_key: modified_key.clone(),
                    physical_key: *physical_key,
                    modifiers: *modifiers,
                    text: text.clone(),
                    status: Status::Focused {
                        is_hovered: cursor.is_over(bounds),
                    },
                };

                self.on_key_press(press, state, renderer, bounds, clipboard, shell);
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _defaults: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let mut internal = self.content.0.borrow_mut();
        let internal = &mut *internal;

        let is_hovered = cursor.is_over(bounds);
        let status = if self.on_action.is_none() {
            Status::Disabled
        } else if state.focus.is_some() {
            Status::Focused { is_hovered }
        } else if is_hovered {
            Status::Hovered
        } else {
            Status::Active
        };

        let style = theme.style(&self.class, status);
        let metrics = self.metrics(renderer, bounds, internal.buffer.line_count());

        let mut cache = state.cache.borrow_mut();
        cache.set_syntax(&style.syntax, &internal.buffer);
        self.sync(internal, &mut cache, &metrics);

        // Track the widest visible line to bound horizontal scrolling.
        if !self.wrap {
            let visible = visible_lines(
                &cache,
                internal.scroll,
                &metrics,
                internal.buffer.line_count(),
            );

            for line in visible {
                let width = cache.paragraph(&internal.buffer, line).min_bounds().width;
                internal.view.max_line_width = internal.view.max_line_width.max(width);
            }
        }

        // Reveal-on-caret-move runs in `update` on `RedrawRequested`, which
        // always precedes a draw; here only clamping remains.
        internal.clamp_scroll();
        let scroll = state.smooth_scroll.borrow_mut().current(internal.scroll);
        cache.measure_visible(
            &internal.buffer,
            scroll.y,
            metrics.rows_in_view(),
            metrics.line_height,
        );

        renderer.fill_quad(
            Quad {
                bounds,
                border: style.border,
                ..Quad::default()
            },
            style.background,
        );

        let buffer = &internal.buffer;
        let caret = buffer.clamp(internal.selection.head);
        let caret_line = buffer.byte_to_line(caret);
        let visible = visible_lines(&cache, scroll, &metrics, buffer.line_count());
        let marks = Marks {
            search_matches: internal.resolved_search_matches(),
            current_search_match: internal.current_search_match,
            delimiter_matches: matching_delimiter_ranges(buffer, caret),
            diagnostics: internal.resolved_diagnostics(),
        };

        let frame = Frame {
            metrics: &metrics,
            scroll,
            visible: &visible,
        };

        if let Some(clip) = metrics.text_area.intersection(viewport) {
            self.draw_text_area(
                renderer, state, internal, &mut cache, &frame, &style, &marks, clip,
            );
        }

        self.draw_gutter(renderer, &cache, &frame, &style, caret_line, viewport);

        if self.scrollbar {
            self.draw_scrollbars(
                renderer, state, internal, &cache, &frame, &style, &marks, bounds, viewport, cursor,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.on_action.is_none() {
            return mouse::Interaction::None;
        }

        let bounds = layout.bounds();
        let Some(position) = cursor.position_over(bounds) else {
            return mouse::Interaction::None;
        };

        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();

        // An ongoing drag answers without touching the cache: this runs on
        // every mouse move.
        if state.scrollbar_drag.is_some() {
            return mouse::Interaction::Pointer;
        }

        self.with_layout(state, renderer, bounds, |_, metrics, cache, scroll| {
            let over_scrollbar = self.scrollbar && {
                let (vertical, horizontal) = scrollbar_geometries(
                    bounds,
                    metrics,
                    self.wrap,
                    metrics.content_height(cache.total_rows()),
                    cache.content_width(),
                    scroll,
                );
                [vertical, horizontal]
                    .into_iter()
                    .flatten()
                    .any(|geometry| geometry.track.contains(position))
            };

            if over_scrollbar || fold_marker_at(position, metrics, scroll, cache).is_some() {
                mouse::Interaction::Pointer
            } else if metrics
                .gutter
                .is_some_and(|gutter| gutter.contains(position))
            {
                mouse::Interaction::Idle
            } else {
                mouse::Interaction::Text
            }
        })
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let on_action = self.on_action.as_deref()?;
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let bounds = layout.bounds();

        // The caret rectangle in window coordinates, anchoring both overlays.
        let (caret, popup_metrics) = self.with_layout(
            state,
            renderer,
            bounds,
            |internal, metrics, cache, scroll| {
                let (line, row, x) = caret_geometry(internal, cache);
                let row_top = (cache.first_row(line) + u64::from(row)) as f32 * metrics.line_height;

                (
                    Rectangle {
                        x: metrics.text_area.x + x - scroll.x + translation.x,
                        y: metrics.text_area.y + row_top - scroll.y + translation.y,
                        width: 2.0,
                        height: metrics.line_height,
                    },
                    PopupMetrics {
                        font: metrics.font,
                        size: metrics.size,
                        line_height: metrics.line_height,
                    },
                )
            },
        );

        let items = state
            .completion
            .as_ref()
            .map(|ui| self.completion_items(ui))
            .unwrap_or_default();

        if !items.is_empty() {
            // Clamp the selection to the (possibly newly delivered) item set.
            if let Some(ui) = &mut state.completion {
                ui.select(ui.selected, items.len());
            }

            return Some(overlay::Element::new(Box::new(CompletionPopup {
                slot: &mut state.completion,
                items,
                on_action,
                caret,
                metrics: popup_metrics,
                class: &self.class,
                _renderer: PhantomData,
            })));
        }

        if let HoverPhase::Shown { at, content, .. } = &state.hover {
            return Some(overlay::Element::new(Box::new(Tooltip {
                content: content.clone(),
                anchor: *at + translation,
                metrics: popup_metrics,
                class: &self.class,
                _renderer: PhantomData,
            })));
        }

        None
    }
}

impl<'a, Message, Theme, Renderer> CodeEditor<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font> + svg::Renderer,
{
    /// Everything painted inside the text area: the current-line highlight,
    /// fold guides, match highlights, the selection, the text itself, the
    /// diagnostic squiggles, and the caret or IME preedit.
    #[allow(clippy::too_many_arguments)]
    fn draw_text_area(
        &self,
        renderer: &mut Renderer,
        state: &State<Renderer::Paragraph>,
        internal: &Internal,
        cache: &mut LineCache<Renderer::Paragraph>,
        frame: &Frame<'_>,
        style: &Style,
        marks: &Marks,
        clip: Rectangle,
    ) {
        let Frame {
            metrics,
            scroll,
            visible,
        } = *frame;
        let buffer = &internal.buffer;
        let selection = internal.selection.range();
        let caret_line = buffer.byte_to_line(buffer.clamp(internal.selection.head));
        let line_y = |cache: &LineCache<Renderer::Paragraph>, line: usize| {
            metrics.text_area.y + cache.first_row(line) as f32 * metrics.line_height - scroll.y
        };

        renderer.start_layer(clip);

        renderer.start_layer(clip);

        // Current line highlight, spanning all of its rows.
        if let Some(color) = style.current_line {
            if visible.binary_search(&caret_line).is_ok() {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: metrics.text_area.x,
                            y: line_y(cache, caret_line),
                            width: metrics.text_area.width,
                            height: cache.rows(caret_line) as f32 * metrics.line_height,
                        },
                        ..Quad::default()
                    },
                    color,
                );
            }
        }

        // Fold guides live with the code, at the indentation of each
        // block. Draw the current (innermost) guide last so it wins when
        // ranges overlap.
        if self.fold_guides != FoldGuides::None && !visible.is_empty() {
            let first = *visible.first().unwrap();
            let last = *visible.last().unwrap();

            // Resolve which guides to draw before handing the cache
            // mutably to the drawing helper; only the visible guides of
            // the `All` mode need collecting.
            let current = cache
                .foldable()
                .iter()
                .filter(|fold| {
                    !cache.is_folded(fold.start)
                        && fold.start <= caret_line
                        && caret_line <= fold.end
                })
                .min_by_key(|fold| fold.end - fold.start)
                .copied();

            let all: Vec<Fold> = if self.fold_guides == FoldGuides::All {
                cache
                    .foldable()
                    .iter()
                    .filter(|fold| {
                        !cache.is_folded(fold.start)
                            && Some(**fold) != current
                            && fold.end >= first
                            && fold.start < last
                    })
                    .copied()
                    .collect()
            } else {
                Vec::new()
            };

            for fold in all {
                draw_fold_guide(
                    renderer,
                    cache,
                    buffer,
                    metrics,
                    scroll,
                    fold,
                    style.fold_guide,
                );
            }

            if let Some(fold) = current {
                draw_fold_guide(
                    renderer,
                    cache,
                    buffer,
                    metrics,
                    scroll,
                    fold,
                    style.fold_guide_current,
                );
            }
        }

        draw_range_highlights(
            renderer,
            buffer,
            cache,
            frame,
            marks
                .search_matches
                .iter()
                .enumerate()
                .filter_map(|(index, range)| {
                    (Some(index) != marks.current_search_match).then_some(range)
                }),
            style.search_match,
        );

        if let Some(range) = marks
            .current_search_match
            .and_then(|index| marks.search_matches.get(index))
        {
            draw_range_highlights(
                renderer,
                buffer,
                cache,
                frame,
                std::iter::once(range),
                style.current_search_match,
            );
        }

        draw_range_highlights(
            renderer,
            buffer,
            cache,
            frame,
            marks.delimiter_matches.iter(),
            style.delimiter_match,
        );

        // Selection, row by row. The newline extension makes a selected
        // line break visible.
        if !selection.is_empty() {
            for &line in visible {
                let top = line_y(cache, line);

                for_each_range_segment(
                    buffer,
                    cache,
                    line,
                    &selection,
                    metrics.size * 0.5,
                    |row, x0, x1| {
                        renderer.fill_quad(
                            Quad {
                                bounds: Rectangle {
                                    x: metrics.text_area.x + x0 - scroll.x,
                                    y: top + row as f32 * metrics.line_height,
                                    width: x1 - x0,
                                    height: metrics.line_height,
                                },
                                ..Quad::default()
                            },
                            style.selection,
                        );
                    },
                );
            }
        }

        // Text. Hanging-indented lines use individually shaped visual rows:
        // row 0 can use the full width, continuation rows fit after the
        // indent, and drawing never paints the same wrapped paragraph twice
        // through adjoining clips (which produced seams at fractional scales).
        for &line in visible {
            let top = line_y(cache, line);
            let indent = cache.line_indent(buffer, line);
            let rows = cache.rows(line);
            let left = metrics.text_area.x - scroll.x;

            if indent > 0.0 && rows > 1 {
                for (row, paragraph) in cache.row_paragraphs(buffer, line).iter().enumerate() {
                    let shift = if row == 0 { 0.0 } else { indent };
                    renderer.fill_paragraph(
                        paragraph,
                        Point::new(left + shift, top + row as f32 * metrics.line_height),
                        style.text,
                        clip,
                    );
                }
            } else {
                let paragraph = cache.paragraph(buffer, line);
                renderer.fill_paragraph(paragraph, Point::new(left, top), style.text, clip);
            }
        }

        // Diagnostic squiggles, row by row, under the text.
        for (range, severity) in &marks.diagnostics {
            let color = style.diagnostic.color(*severity);
            let first = buffer.byte_to_line(range.start);
            let last = buffer.byte_to_line(range.end);

            for line in (first..=last).filter(|line| visible.binary_search(line).is_ok()) {
                let content = buffer.line_content_range(line);

                // A zero-width diagnostic still marks one grapheme. At
                // the very end of the line, where there is none left, it
                // stays empty and gets a caret-width slot below.
                let range = if range.is_empty() {
                    let start = range.start.clamp(content.start, content.end);
                    start..next_grapheme_end(buffer, start, content.end)
                } else {
                    range.clone()
                };

                let top = line_y(cache, line);
                let slot = range.is_empty();

                for_each_range_segment(buffer, cache, line, &range, 0.0, |row, x0, x1| {
                    let x1 = if slot { x0 + metrics.size * 0.5 } else { x1 };

                    draw_squiggle(
                        renderer,
                        metrics.text_area.x + x0 - scroll.x,
                        metrics.text_area.x + x1 - scroll.x,
                        top + (row as f32 + 1.0) * metrics.line_height - 2.0,
                        color,
                    );
                });
            }
        }

        // Caret and preedit.
        let show_caret = state
            .focus
            .as_ref()
            .is_some_and(|focus| focus.is_cursor_visible());

        let preedit = state
            .preedit
            .as_ref()
            .filter(|preedit| !preedit.content.is_empty())
            .filter(|_| state.focus.is_some());

        if (show_caret || preedit.is_some()) && visible.binary_search(&caret_line).is_ok() {
            let (_, row, x) = caret_geometry(internal, cache);

            let caret_position = Point::new(
                metrics.text_area.x + x - scroll.x,
                line_y(cache, caret_line) + row as f32 * metrics.line_height,
            );

            if let Some(preedit) = preedit {
                draw_preedit(renderer, preedit, caret_position, metrics, style, clip);
            } else {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: caret_position.x,
                            y: caret_position.y,
                            width: 2.0,
                            height: metrics.line_height,
                        },
                        ..Quad::default()
                    },
                    style.cursor,
                );
            }
        }

        renderer.end_layer();

        renderer.end_layer();
    }

    /// The line-number gutter with its fold markers.
    fn draw_gutter(
        &self,
        renderer: &mut Renderer,
        cache: &LineCache<Renderer::Paragraph>,
        frame: &Frame<'_>,
        style: &Style,
        caret_line: usize,
        viewport: &Rectangle,
    ) {
        let Frame {
            metrics,
            scroll,
            visible,
        } = *frame;
        let Some((gutter, gutter_layout)) = metrics.gutter.zip(metrics.gutter_layout()) else {
            return;
        };
        let line_y = |cache: &LineCache<Renderer::Paragraph>, line: usize| {
            metrics.text_area.y + cache.first_row(line) as f32 * metrics.line_height - scroll.y
        };

        if let Some(clip) = gutter.intersection(viewport) {
            renderer.start_layer(clip);

            for &line in visible {
                let color = if line == caret_line {
                    style.gutter_current_text
                } else {
                    style.gutter_text
                };

                if cache.has_fold(line) {
                    let (bytes, handle) = if cache.is_folded(line) {
                        (FOLD_COLLAPSED, &FOLD_COLLAPSED_HANDLE)
                    } else {
                        (FOLD_EXPANDED, &FOLD_EXPANDED_HANDLE)
                    };
                    let handle = handle
                        .get_or_init(|| svg::Handle::from_memory(bytes))
                        .clone();
                    let size = metrics.size.min(metrics.line_height);

                    renderer.draw_svg(
                        svg::Svg::new(handle).color(color),
                        Rectangle {
                            x: gutter_layout.marker.center_x() - size / 2.0,
                            y: line_y(cache, line) + (metrics.line_height - size) / 2.0,
                            width: size,
                            height: size,
                        },
                        clip,
                    );
                }

                renderer.fill_text(
                    Text {
                        content: (line + 1).to_string(),
                        bounds: Size::new(gutter.width, metrics.line_height),
                        size: Pixels(metrics.size),
                        line_height: text::LineHeight::Absolute(Pixels(metrics.line_height)),
                        font: metrics.font,
                        align_x: text::Alignment::Right,
                        align_y: alignment::Vertical::Top,
                        shaping: text::Shaping::Basic,
                        wrapping: text::Wrapping::None,
                    },
                    Point::new(gutter_layout.number_right, line_y(cache, line)),
                    color,
                    clip,
                );
            }

            renderer.end_layer();
        }
    }

    /// The editor-owned scrollbars with their diagnostic/search tick marks.
    #[allow(clippy::too_many_arguments)]
    fn draw_scrollbars(
        &self,
        renderer: &mut Renderer,
        state: &State<Renderer::Paragraph>,
        internal: &Internal,
        cache: &LineCache<Renderer::Paragraph>,
        frame: &Frame<'_>,
        style: &Style,
        marks: &Marks,
        bounds: Rectangle,
        viewport: &Rectangle,
        cursor: mouse::Cursor,
    ) {
        let Frame {
            metrics, scroll, ..
        } = *frame;

        let dragging_axis = state.scrollbar_drag.map(|drag| drag.geometry.axis);

        // Freeze the vertical extent during a vertical drag so lazily
        // measured rows cannot resize the thumb under the pointer.
        let content_height = match state.scrollbar_drag {
            Some(drag) if drag.geometry.axis == Axis::Vertical => drag.geometry.content_extent,
            _ => metrics.content_height(cache.total_rows()),
        };

        let (vertical, horizontal) = scrollbar_geometries(
            bounds,
            metrics,
            self.wrap,
            content_height,
            cache.content_width(),
            scroll,
        );

        if let Some(clip) = bounds.intersection(viewport) {
            let hovered = |geometry: &ScrollbarGeometry| {
                cursor
                    .position_over(bounds)
                    .is_some_and(|position| geometry.track.contains(position))
            };

            renderer.start_layer(clip);

            if let Some(geometry) = vertical {
                let dragging = dragging_axis == Some(Axis::Vertical);
                let ticks = scrollbar_ticks(
                    &geometry,
                    &internal.buffer,
                    cache,
                    style,
                    &marks.diagnostics,
                    &marks.search_matches,
                    marks.current_search_match,
                );
                draw_scrollbar(
                    renderer,
                    &geometry,
                    style,
                    dragging || hovered(&geometry),
                    dragging,
                    &ticks,
                );
            }

            if let Some(geometry) = horizontal {
                let dragging = dragging_axis == Some(Axis::Horizontal);
                draw_scrollbar(
                    renderer,
                    &geometry,
                    style,
                    dragging || hovered(&geometry),
                    dragging,
                    &[],
                );
            }

            renderer.end_layer();
        }
    }
}

impl<'a, Message, Theme, Renderer> CodeEditor<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font>,
{
    /// Handles a left press: scrollbar grabs, fold-marker clicks, and caret
    /// placement with single/double/triple-click semantics.
    fn on_mouse_press(
        &self,
        state: &mut State<Renderer::Paragraph>,
        renderer: &Renderer,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        shell: &mut Shell<'_, Message>,
    ) {
        // Any click dismisses the popup and tooltip.
        state.completion = None;
        state.hover = HoverPhase::Idle;

        let Some(position) = cursor.position_over(bounds) else {
            if state.focus.is_some() {
                state.focus = None;
                state.drag_click = None;
                shell.request_redraw();
            }

            return;
        };

        if self.scrollbar {
            // Hit-test against the displayed (smooth-scroll) offset,
            // matching where the thumb is actually drawn.
            let handled = self.with_layout(
                state,
                renderer,
                bounds,
                |internal, metrics, cache, scroll| {
                    let (vertical, horizontal) = scrollbar_geometries(
                        bounds,
                        metrics,
                        self.wrap,
                        metrics.content_height(cache.total_rows()),
                        cache.content_width(),
                        scroll,
                    );

                    [vertical, horizontal]
                        .into_iter()
                        .flatten()
                        .find(|geometry| geometry.track.contains(position))
                        .map(|geometry| {
                            let (drag, target) = begin_scrollbar_drag(geometry, position);
                            let (action, scroll) =
                                scroll_to(geometry.axis, target, internal.scroll);

                            (drag, action, scroll)
                        })
                },
            );

            if let Some((drag, action, scroll)) = handled {
                state.scrollbar_drag = Some(drag);
                state.smooth_scroll.borrow_mut().jump_to(scroll);

                shell.publish((self.on_action.as_ref().unwrap())(action));
                shell.capture_event();
                shell.request_redraw();
                return;
            }
        }

        if let Some(line) =
            self.with_layout(state, renderer, bounds, |_, metrics, cache, scroll| {
                fold_marker_at(position, metrics, scroll, cache)
            })
        {
            state.focus = Some(Focus::now());
            state.last_click = Some(mouse::Click::new(
                position,
                mouse::Button::Left,
                state.last_click,
            ));
            state.drag_click = None;
            state.goal_x = None;

            shell.publish((self.on_action.as_ref().unwrap())(Action::ToggleFold(line)));
            shell.capture_event();
            shell.request_redraw();
            return;
        }

        let offset = self.with_layout(
            state,
            renderer,
            bounds,
            |internal, metrics, cache, scroll| {
                offset_at(position, metrics, internal, cache, scroll)
            },
        );

        let click = mouse::Click::new(position, mouse::Button::Left, state.last_click);

        let action = match click.kind() {
            mouse::click::Kind::Single if state.modifiers.shift() => Action::SelectTo(offset),
            mouse::click::Kind::Single => Action::MoveTo(offset),
            mouse::click::Kind::Double => Action::SelectWord(offset),
            mouse::click::Kind::Triple => Action::SelectLine(offset),
        };

        state.focus = Some(Focus::now());
        state.last_click = Some(click);
        state.drag_click = Some(click.kind());
        state.goal_x = None;

        shell.publish((self.on_action.as_ref().unwrap())(action));
        shell.capture_event();
    }

    fn open_context_menu(
        &self,
        state: &mut State<Renderer::Paragraph>,
        renderer: &Renderer,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        shell: &mut Shell<'_, Message>,
    ) {
        let Some(on_context_menu) = self.on_context_menu.as_ref() else {
            return;
        };
        let Some(position) = cursor.position_over(bounds) else {
            return;
        };

        state.completion = None;
        state.hover = HoverPhase::Idle;
        state.focus = Some(Focus::now());
        state.drag_click = None;
        state.goal_x = None;

        let offset = self.with_layout(
            state,
            renderer,
            bounds,
            |internal, metrics, cache, scroll| {
                offset_at(position, metrics, internal, cache, scroll)
            },
        );

        shell.publish(on_context_menu(position, offset));
        shell.capture_event();
        shell.request_redraw();
    }

    /// Resolves a key press to a [`Binding`], lets a visible completion
    /// popup capture its navigation keys first, applies the binding, and
    /// settles what the popup does afterwards.
    fn on_key_press(
        &self,
        press: KeyPress,
        state: &mut State<Renderer::Paragraph>,
        renderer: &Renderer,
        bounds: Rectangle,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        // While the popup is visible, it captures navigation keys. An
        // open session whose items have not arrived yet shows nothing,
        // so keys must keep their editing meaning — otherwise an Enter
        // typed right after a trigger character would be swallowed.
        let popup_visible = state
            .completion
            .as_ref()
            .is_some_and(|ui| self.has_completion_items(ui));

        if popup_visible {
            if let Some(nav) = completion_nav(&press.modified_key, press.modifiers) {
                self.navigate_completion(nav, state, shell);
                shell.request_redraw();
                shell.capture_event();
                return;
            }
        }

        let binding = match &self.key_binding {
            Some(key_binding) => key_binding(press),
            None => Binding::from_key_press(press),
        };

        let Some(binding) = binding else {
            return;
        };

        let unfocuses = matches!(binding, Binding::Unfocus);
        // `Complete` opens the popup, so it must not trigger the
        // post-effect that would immediately close it.
        let opens = matches!(binding, Binding::Complete);
        let effect = completion_effect(&binding);
        let trigger = self.binding_completion_trigger(&binding).map(|trigger| {
            let offset = self.content.0.borrow().selection.head;
            (offset, trigger)
        });

        self.apply_binding(binding, state, renderer, bounds, clipboard, shell);

        if let Some(trigger) = trigger {
            self.open_completion(state, Some(trigger));
            shell.request_redraw();
        }

        // Typing keeps the popup but queues a fresh request; a caret
        // move closes it.
        if !opens && trigger.is_none() {
            if let Some(ui) = &mut state.completion {
                match effect {
                    CompletionEffect::Refresh => {
                        ui.needs_request = true;
                        shell.request_redraw();
                    }
                    CompletionEffect::Close => state.completion = None,
                    CompletionEffect::Keep => {}
                }
            }
        }

        if !unfocuses {
            if let Some(focus) = &mut state.focus {
                focus.updated_at = Instant::now();
            }

            shell.capture_event();
        }
    }

    /// Carries out a resolved [`Binding`].
    fn apply_binding(
        &self,
        binding: Binding<Message>,
        state: &mut State<Renderer::Paragraph>,
        renderer: &Renderer,
        bounds: Rectangle,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let on_action = self.on_action.as_ref().unwrap();

        match binding {
            Binding::Unfocus => {
                state.focus = None;
                state.drag_click = None;
                // The completion session is caret-bound; without focus there
                // is no caret, and late-arriving items must not pop it open.
                state.completion = None;
                shell.request_redraw();
            }
            Binding::Copy => {
                if let Some(selection) = self.content.selection_text() {
                    clipboard.write(clipboard::Kind::Standard, selection);
                }
            }
            Binding::Cut => {
                if let Some(selection) = self.content.selection_text() {
                    clipboard.write(clipboard::Kind::Standard, selection);
                    shell.publish(on_action(Action::Delete));
                }
            }
            Binding::Paste => {
                if let Some(contents) = clipboard.read(clipboard::Kind::Standard) {
                    state.goal_x = None;
                    shell.publish(on_action(Action::Paste(contents)));
                }
            }
            Binding::Action(action) => {
                let action = self.drive_vertical(action, state, renderer, bounds);
                shell.publish(on_action(action));
            }
            Binding::Complete => {
                self.open_completion(state, None);
                shell.request_redraw();
            }
            Binding::Custom(message) => {
                shell.publish(message);
            }
            Binding::Sequence(sequence) => {
                for binding in sequence {
                    self.apply_binding(binding, state, renderer, bounds, clipboard, shell);
                }
            }
        }
    }

    /// Opens the completion popup, flagging that a request must be emitted.
    fn open_completion(
        &self,
        state: &mut State<Renderer::Paragraph>,
        trigger: Option<(usize, char)>,
    ) {
        if !self.completion_enabled {
            return;
        }

        state.completion = Some(CompletionUi {
            selected: 0,
            scroll: 0,
            // Only results from requests made from now on are shown.
            session_min_id: state.next_request_id,
            needs_request: true,
            trigger,
        });
    }

    fn binding_completion_trigger(&self, binding: &Binding<Message>) -> Option<char> {
        if !self.completion_enabled || self.completion_triggers.is_empty() {
            return None;
        }

        match binding {
            Binding::Action(Action::Insert(text)) => {
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) if self.completion_triggers.contains(&ch) => Some(ch),
                    _ => None,
                }
            }
            Binding::Sequence(sequence) => sequence
                .iter()
                .find_map(|binding| self.binding_completion_trigger(binding)),
            _ => None,
        }
    }

    /// Emits a queued completion request for the current caret, if the popup
    /// is open and marked for refresh. Runs on redraw, once the content
    /// reflects the latest edit.
    fn emit_completion_request(
        &self,
        state: &mut State<Renderer::Paragraph>,
        shell: &mut Shell<'_, Message>,
    ) {
        let Some(ui) = &mut state.completion else {
            return;
        };

        if let Some((offset, trigger)) = ui.trigger {
            let internal = self.content.0.borrow();
            let caret = internal.selection.head;
            if !automatic_completion_is_valid(internal.buffer.text(), caret, offset, trigger) {
                drop(internal);
                state.completion = None;
                shell.request_redraw();
                return;
            }
        }

        if !ui.needs_request {
            return;
        }

        ui.needs_request = false;
        let id = state.next_request_id;
        state.next_request_id += 1;

        let offset = self.content.0.borrow().selection.head;
        let on_action = self.on_action.as_ref().unwrap();
        shell.publish(on_action(Action::RequestCompletions {
            id,
            offset,
            explicit: ui.trigger.is_none(),
        }));
    }

    /// Whether any delivered completions would currently be displayed,
    /// without cloning them.
    fn has_completion_items(&self, ui: &CompletionUi) -> bool {
        let internal = self.content.0.borrow();
        let caret = internal.selection.head;

        internal.completions.as_ref().is_some_and(|(id, items)| {
            *id >= ui.session_min_id
                && items
                    .iter()
                    .any(|item| completion_for_caret(internal.buffer.text(), caret, item).is_some())
        })
    }

    /// The delivered completions this popup should display: the latest
    /// results, as long as they belong to this popup session.
    fn completion_items(&self, ui: &CompletionUi) -> Vec<Completion> {
        let internal = self.content.0.borrow();
        let caret = internal.selection.head;

        match &internal.completions {
            Some((id, items)) if *id >= ui.session_min_id => items
                .iter()
                .filter_map(|item| completion_for_caret(internal.buffer.text(), caret, item))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Applies a popup navigation key.
    fn navigate_completion(
        &self,
        nav: CompletionNav,
        state: &mut State<Renderer::Paragraph>,
        shell: &mut Shell<'_, Message>,
    ) {
        let count = match &state.completion {
            Some(ui) => self.completion_items(ui).len(),
            None => return,
        };

        match nav {
            CompletionNav::Previous => {
                if let Some(ui) = &mut state.completion {
                    ui.select_previous(count);
                }
            }
            CompletionNav::Next => {
                if let Some(ui) = &mut state.completion {
                    ui.select_next(count);
                }
            }
            CompletionNav::Dismiss => state.completion = None,
            CompletionNav::Accept => {
                if let Some(ui) = &state.completion {
                    let items = self.completion_items(ui);
                    if let Some(item) = items.get(ui.selected) {
                        let on_action = self.on_action.as_ref().unwrap();
                        shell.publish(on_action(Action::Replace {
                            range: item.replace.clone(),
                            text: item.insert.clone(),
                        }));
                    }
                }
                state.completion = None;
            }
        }
    }

    /// Advances the hover lifecycle: once the pointer has rested long enough,
    /// show a diagnostic message directly, or emit an async hover request and
    /// wait for the delivered result. Returns whether the tooltip changed.
    fn resolve_hover(
        &self,
        state: &mut State<Renderer::Paragraph>,
        renderer: &Renderer,
        bounds: Rectangle,
        now: Instant,
        shell: &mut Shell<'_, Message>,
    ) -> bool {
        // A delivered hover result completes a pending request.
        if let HoverPhase::Requested { at, id } = &state.hover {
            let (at, id) = (*at, *id);

            return match self.content.hover() {
                Some((delivered, hover)) if delivered == id => {
                    state.hover = match hover {
                        Some(hover) => HoverPhase::Shown {
                            at,
                            content: hover.content,
                        },
                        None => HoverPhase::Idle,
                    };
                    true
                }
                _ => false,
            };
        }

        let HoverPhase::Pending { at, since } = &state.hover else {
            return false;
        };

        let (at, since) = (*at, *since);

        if now.saturating_duration_since(since) < self.hover_delay {
            return false;
        }

        let offset = self.with_layout(
            state,
            renderer,
            bounds,
            |internal, metrics, cache, scroll| {
                hover_offset_at(at, metrics, internal, cache, scroll)
            },
        );

        let Some(offset) = offset else {
            state.hover = HoverPhase::Idle;
            return true;
        };

        // A diagnostic under the pointer is resolved synchronously and wins.
        if let Some((_range, message)) = self.content.0.borrow().diagnostic_at(offset) {
            state.hover = HoverPhase::Shown {
                at,
                content: message,
            };
            return true;
        }

        // Otherwise ask the application, asynchronously.
        if self.hover_enabled {
            let id = state.next_request_id;
            state.next_request_id += 1;
            state.hover = HoverPhase::Requested { at, id };

            let on_action = self.on_action.as_ref().unwrap();
            shell.publish(on_action(Action::RequestHover { id, offset }));
            true
        } else {
            state.hover = HoverPhase::Idle;
            true
        }
    }

    /// Turns vertical motions into explicit offsets, moving by *visual* row
    /// so that movement follows soft wrap. Other actions pass through and
    /// reset the goal column.
    fn drive_vertical(
        &self,
        action: Action,
        state: &mut State<Renderer::Paragraph>,
        renderer: &Renderer,
        bounds: Rectangle,
    ) -> Action {
        let (motion, extend) = match &action {
            Action::Move(motion) => (*motion, false),
            Action::Select(motion) => (*motion, true),
            _ => {
                state.goal_x = None;
                return action;
            }
        };

        // Rows per viewport page are only known inside the layout closure.
        let (rows, pages) = match motion {
            Motion::Up => (-1, 0),
            Motion::Down => (1, 0),
            Motion::PageUp => (0, -1),
            Motion::PageDown => (0, 1),
            _ => {
                state.goal_x = None;
                return action;
            }
        };

        let goal = state.goal_x;

        let (offset, goal) =
            self.with_layout(state, renderer, bounds, |internal, metrics, cache, _| {
                let delta = rows + pages * metrics.rows_in_view() as i64;

                let (line, row, x) = caret_geometry(internal, cache);
                let goal = goal.unwrap_or(x);

                let current = cache.first_row(line) as i64 + i64::from(row);
                let target = current + delta;

                let offset = if target < 0 {
                    0
                } else if target >= cache.total_rows() as i64 {
                    internal.buffer.len()
                } else {
                    offset_at_row(
                        Point::new(goal, 0.0),
                        target as u64,
                        metrics,
                        &internal.buffer,
                        cache,
                    )
                };

                (offset, goal)
            });

        state.goal_x = Some(goal);

        if extend {
            Action::SelectTo(offset)
        } else {
            Action::MoveTo(offset)
        }
    }
}

/// The action and scroll vector for an absolute scroll along one axis,
/// keeping the other axis at `current`.
fn scroll_to(axis: Axis, target: f32, current: Vector) -> (Action, Vector) {
    match axis {
        Axis::Vertical => (
            Action::ScrollTo {
                x: None,
                y: Some(target),
            },
            Vector::new(current.x, target),
        ),
        Axis::Horizontal => (
            Action::ScrollTo {
                x: Some(target),
                y: None,
            },
            Vector::new(target, current.y),
        ),
    }
}

fn fold_marker_at<P: text::Paragraph<Font = Font>>(
    position: Point,
    metrics: &Metrics,
    scroll: Vector,
    cache: &LineCache<P>,
) -> Option<usize> {
    if !metrics.gutter_layout()?.marker.contains(position) {
        return None;
    }

    let y = position.y - metrics.text_area.y + scroll.y;
    let total = cache.total_rows().max(1);
    let row = ((y / metrics.line_height).floor().max(0.0) as u64).min(total - 1);
    let line = cache.line_at_row(row);

    cache.has_fold(line).then_some(line)
}

fn automatic_completion_is_valid(
    text: &str,
    caret: usize,
    trigger_offset: usize,
    trigger: char,
) -> bool {
    trigger_offset < caret
        && text
            .get(trigger_offset..caret)
            .is_some_and(|prefix| prefix.starts_with(trigger))
}

fn completion_for_caret(text: &str, caret: usize, item: &Completion) -> Option<Completion> {
    if caret < item.replace.start {
        return None;
    }

    let prefix = text.get(item.replace.start..caret)?;
    if !prefix.is_empty() && !item.label.starts_with(prefix) {
        return None;
    }

    let mut item = item.clone();
    item.replace.end = caret;
    Some(item)
}

/// A popup navigation key.
enum CompletionNav {
    Previous,
    Next,
    Accept,
    Dismiss,
}

/// What a key press does to an open popup afterwards.
enum CompletionEffect {
    /// The edit changed the text: recompute the candidates.
    Refresh,
    /// The caret moved away: close the popup.
    Close,
    /// Leave the popup as it is.
    Keep,
}

/// Maps a key to a popup navigation action, while the popup is open.
fn completion_nav(
    modified_key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
) -> Option<CompletionNav> {
    use keyboard::key::Named;

    match modified_key.as_ref() {
        keyboard::Key::Named(Named::ArrowUp) => Some(CompletionNav::Previous),
        keyboard::Key::Named(Named::ArrowDown) => Some(CompletionNav::Next),
        keyboard::Key::Named(Named::Enter | Named::Tab) if !modifiers.shift() => {
            Some(CompletionNav::Accept)
        }
        keyboard::Key::Named(Named::Escape) => Some(CompletionNav::Dismiss),
        _ => None,
    }
}

/// Decides what happens to an open popup after a binding is applied.
fn completion_effect<Message>(binding: &Binding<Message>) -> CompletionEffect {
    match binding {
        Binding::Action(action) if action.is_edit() => CompletionEffect::Refresh,
        Binding::Paste | Binding::Cut => CompletionEffect::Refresh,
        Binding::Action(_) => CompletionEffect::Close,
        Binding::Copy | Binding::Custom(_) => CompletionEffect::Keep,
        // The steps run in order and nothing reopens a closed popup, so the
        // sequence closes it if any step does, and refreshes it if any step
        // edits: the strongest effect wins.
        Binding::Sequence(sequence) => sequence.iter().map(completion_effect).fold(
            CompletionEffect::Keep,
            |combined, effect| match (combined, effect) {
                (CompletionEffect::Close, _) | (_, CompletionEffect::Close) => {
                    CompletionEffect::Close
                }
                (CompletionEffect::Refresh, _) | (_, CompletionEffect::Refresh) => {
                    CompletionEffect::Refresh
                }
                _ => CompletionEffect::Keep,
            },
        ),
        _ => CompletionEffect::Close,
    }
}

impl<'a, Message, Theme, Renderer> From<CodeEditor<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer<Font = Font> + svg::Renderer + 'a,
{
    fn from(editor: CodeEditor<'a, Message, Theme, Renderer>) -> Self {
        Self::new(editor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_completion_items_are_filtered_by_the_current_prefix() {
        let figure = Completion::new(1..1, "figure");
        let text = Completion::new(1..1, "text");

        assert_eq!(
            completion_for_caret("#fig", 4, &figure).map(|item| item.replace),
            Some(1..4)
        );
        assert!(completion_for_caret("#fig", 4, &text).is_none());
    }

    #[test]
    fn deleting_an_automatic_trigger_invalidates_the_session() {
        assert!(automatic_completion_is_valid("#fig", 4, 0, '#'));
        assert!(!automatic_completion_is_valid("fig", 3, 0, '#'));
        assert!(!automatic_completion_is_valid("", 0, 0, '#'));
    }
}
