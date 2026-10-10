//! Product presentation for native Kit controls. State and callbacks stay with callers.
use crate::ui::theme::{self, metrics as m, FONT_MONO};
use gpui::{
    div, point, prelude::FluentBuilder, px, App, BoxShadow, ElementId, Entity, Focusable,
    InteractiveElement, IntoElement, ParentElement, SharedString, Styled, Window,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    dialog::Dialog,
    input::{Input, InputGroup, InputGroupAddon, InputGroupAddonAlignment, InputState},
    searchable_list::SearchableVec,
    select::{Select, SelectState},
    tab::{Tab, TabBar},
    ActiveTheme, FocusableExt, Icon,
};

pub type MethodState = SelectState<SearchableVec<&'static str>>;

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id)
        .label(label)
        .h(m::CONTROL)
        .px_3()
        .text_size(m::LABEL)
        .font_weight(m::MEDIUM)
}

pub fn icon_button(id: impl Into<ElementId>, icon: IconName, label: &'static str) -> Button {
    Button::new(id)
        .ghost()
        .icon(Icon::new(icon).size(m::SMALL_ICON))
        .accessibility_label(label)
        .tooltip(label)
        .size(m::ICON_BUTTON)
}

fn frame(
    id: impl Into<ElementId>,
    input: Input,
    focused: bool,
    invalid: bool,
    disabled: bool,
    cx: &App,
) -> InputGroup {
    let color = if invalid {
        cx.theme().danger
    } else {
        cx.theme().ring
    };
    InputGroup::new(id)
        .input(input)
        .h(m::CONTROL)
        .flex_none()
        .rounded(m::RADIUS)
        .border_color(cx.theme().border)
        .focus_ring(false)
        .bg(cx.theme().background)
        .invalid(invalid)
        .disabled(disabled)
        .when(!disabled && (focused || invalid), |group| {
            group.border_color(color).shadow(focus_shadow(color))
        })
}

fn focus_shadow(color: gpui::Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        inset: false,
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(1.),
    }]
}

pub fn search_input(
    id: impl Into<ElementId>,
    input: Input,
    state: &Entity<InputState>,
    window: &Window,
    cx: &App,
) -> InputGroup {
    frame(
        id,
        input
            .text_size(m::LABEL)
            .font_family(theme::FONT_UI)
            .font_weight(gpui::FontWeight::NORMAL)
            .line_height(gpui::relative(m::LINE_HEIGHT)),
        state.focus_handle(cx).is_focused(window),
        false,
        false,
        cx,
    )
    .addon(
        InputGroupAddon::new("search-icon").child(Icon::new(IconName::Search).size(m::SMALL_ICON)),
    )
}

/// A single frame owns focus/error styling; Kit owns editing and the Select popup.
#[allow(clippy::too_many_arguments)]
pub fn request_url(
    id: impl Into<ElementId>,
    input: Input,
    state: &Entity<InputState>,
    method: &Entity<MethodState>,
    invalid: bool,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> InputGroup {
    let focused =
        state.focus_handle(cx).is_focused(window) || method.focus_handle(cx).is_focused(window);
    let method_color = theme::method_color(
        method
            .read(cx)
            .selected_value()
            .copied()
            .unwrap_or("GET")
            .into(),
    )
    .resolve(cx);
    frame(
        id,
        input
            .font_family(FONT_MONO)
            .text_size(m::CODE)
            .px(m::URL_INSET),
        focused,
        invalid,
        disabled,
        cx,
    )
    .h(m::URL)
    .rounded(m::URL_RADIUS)
    .bg(cx.theme().muted)
    .border_color(cx.theme().input)
    .when(!disabled && (focused || invalid), |group| {
        group.border_color(if invalid {
            cx.theme().danger
        } else {
            cx.theme().ring
        })
    })
    .addon(
        InputGroupAddon::new("method-addon")
            .p_0()
            .w(m::METHOD_WIDTH)
            .flex_none()
            .child(
                Select::new(method)
                    .id("method-select")
                    .accessibility_label("HTTP method")
                    .appearance(false)
                    .focus_ring(false)
                    .disabled(disabled)
                    .h(m::URL - gpui::rems(2. / 16.))
                    .border(px(0.))
                    .border_r_1()
                    .border_color(cx.theme().input)
                    .rounded(px(0.))
                    .w(m::METHOD_WIDTH)
                    .font_family(FONT_MONO)
                    .text_size(m::CODE)
                    .font_weight(m::SEMIBOLD)
                    .text_color(method_color),
            ),
    )
    .addon(
        InputGroupAddon::new("url-lock")
            .align(InputGroupAddonAlignment::InlineEnd)
            .w(gpui::rems(2.))
            .flex_none()
            .child(Icon::new(IconName::Lock).size(m::SMALL_ICON)),
    )
}

pub fn tabs(id: impl Into<ElementId>, labels: &[&'static str], selected: usize) -> TabBar {
    TabBar::new(id)
        .underline()
        .selected_index(selected)
        .h(m::PANE_TAB)
        .flex_none()
        .children(labels.iter().map(|label| {
            Tab::new()
                .label(*label)
                .aria_label(*label)
                .h(m::PANE_TAB)
                .text_size(m::LABEL)
                .font_weight(m::MEDIUM)
        }))
}

/// Component Checkbox fixes its check color to primary (orange). The Kit Base
/// primitive lets the row use the prototype's green mark while retaining Kit's
/// focus, keyboard toggle, disabled semantics, and accessible checked state.
pub fn parameter_checkbox(
    id: impl Into<ElementId>,
    checked: bool,
    cx: &App,
) -> gpui_kit::base::Checkbox {
    let green = theme::OK.resolve(cx);
    let focus = cx.theme().ring;
    gpui_kit::base::Checkbox::new(id)
        .checked(checked)
        .size(gpui::rems(1.))
        .rounded(gpui::rems(0.125))
        .border_1()
        .border_color(if checked {
            green.into()
        } else {
            cx.theme().input
        })
        .when(checked, |checkbox| checkbox.bg(green))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .focus(move |style| style.border_color(focus).shadow(focus_shadow(focus)))
        .when(checked, |checkbox| {
            checkbox.child(
                Icon::new(IconName::Check)
                    .size(gpui::rems(0.75))
                    .text_color(cx.theme().background),
            )
        })
}

pub fn editable_row(
    id: impl Into<ElementId>,
    checkbox: impl IntoElement,
    key: Input,
    value: Input,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(id)
        .h(m::TABLE_ROW)
        .flex()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .w_10()
                .flex_none()
                .flex()
                .justify_center()
                .child(checkbox),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .border_l_1()
                .border_color(cx.theme().border)
                .child(
                    key.appearance(false)
                        .h(m::TABLE_ROW)
                        .font_family(FONT_MONO)
                        .text_size(m::CODE),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .border_l_1()
                .border_color(cx.theme().border)
                .child(
                    value
                        .appearance(false)
                        .h(m::TABLE_ROW)
                        .font_family(FONT_MONO)
                        .text_size(m::CODE),
                ),
        )
}

pub fn dialog(dialog: Dialog, title: &'static str, window: &Window, cx: &App) -> Dialog {
    dialog
        .title(title)
        .width(m::DIALOG_WIDTH.to_pixels(window.rem_size()))
        .rounded(m::DIALOG_RADIUS)
        .text_size(m::BODY)
        .bg(cx.theme().background)
        .border_color(cx.theme().input)
        .shadow(theme::dialog_shadow(cx))
}

/// A Kit Base button for editors that retain a FocusHandle for cell traversal.
/// Component Button 0.7.1 always creates its own handle; its inherited track_focus
/// styles an outer element and cannot replace that handle. Using Base here keeps
/// one focus target while sharing product metrics and Kit activation semantics.
pub fn editor_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &App,
) -> gpui_kit::base::Button {
    editor_button_variant(id, label, false, cx)
}

pub fn editor_primary_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &App,
) -> gpui_kit::base::Button {
    editor_button_variant(id, label, true, cx)
}

fn editor_button_variant(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
    cx: &App,
) -> gpui_kit::base::Button {
    let label = label.into();
    let hover = if primary {
        theme::ACCENT_HOVER
    } else {
        theme::PANEL_ALT
    }
    .resolve(cx);
    let ring = theme::ACCENT.resolve(cx);
    gpui_kit::base::Button::new(id)
        .accessibility_label(label.clone())
        .h(m::CONTROL)
        .px_3()
        .rounded(m::RADIUS)
        .text_size(if primary { m::BODY } else { m::LABEL })
        .font_weight(if primary { m::SEMIBOLD } else { m::MEDIUM })
        .text_color(
            if primary {
                theme::ON_ACCENT
            } else {
                theme::SUBTEXT
            }
            .resolve(cx),
        )
        .when(primary, |b| b.bg(theme::ACCENT.resolve(cx)))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_1().border_color(ring))
        .child(label)
}
