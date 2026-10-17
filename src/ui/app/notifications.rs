use super::*;
use gpui::{Animation, AnimationExt, relative};
use std::time::{Duration, Instant};

const LIFETIME: Duration = Duration::from_secs(3);
const TRANSITION: Duration = Duration::from_millis(180);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NoticeKey {
    revision: u64,
    busy: bool,
    error: bool,
}

impl NoticeKey {
    fn timed(self) -> bool {
        !self.busy && !self.error
    }
}

#[derive(Default)]
pub(super) struct NoticeState {
    key: Option<NoticeKey>,
    generation: usize,
    leaving: bool,
    started_at: Option<Instant>,
}

impl NoticeState {
    fn sync(&mut self, key: NoticeKey) -> bool {
        if self.key == Some(key) {
            return false;
        }
        self.key = Some(key);
        self.generation = self.generation.wrapping_add(1);
        self.leaving = false;
        self.started_at = Some(Instant::now());
        true
    }

    fn accepts(&self, generation: usize, key: NoticeKey) -> bool {
        self.generation == generation && self.key == Some(key)
    }
}

impl Manager {
    pub(super) fn set_message(&mut self, message: String) {
        self.message = message;
        // Repeated messages are distinct notifications and restart the countdown.
        self.message_revision = self.message_revision.wrapping_add(1);
    }

    fn notice_key(&self) -> NoticeKey {
        NoticeKey {
            revision: self.message_revision,
            busy: self.busy,
            error: self.error,
        }
    }

    pub(super) fn refresh_notice(&mut self, cx: &mut Context<Self>) {
        let key = self.notice_key();
        if !self.notice.sync(key) || self.message.is_empty() || !key.timed() {
            return;
        }
        let generation = self.notice.generation;
        let timer = cx.background_executor().timer(LIFETIME);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                if this.notice.accepts(generation, key) && this.notice_key() == key {
                    this.dismiss_notice(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn dismiss_notice(&mut self, cx: &mut Context<Self>) {
        self.refresh_notice(cx);
        if self.message.is_empty() || self.busy || self.notice.leaving {
            return;
        }
        self.notice.leaving = true;
        let generation = self.notice.generation;
        let key = self.notice_key();
        let timer = cx.background_executor().timer(TRANSITION);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                if this.notice.accepts(generation, key)
                    && this.notice_key() == key
                    && this.notice.leaving
                {
                    this.set_message(String::new());
                    this.error = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn task_message(&self, cx: &Context<Self>) -> impl IntoElement {
        let generation = self.notice.generation;
        let leaving = self.notice.leaving;
        let bottom = if self.browser.checked.is_empty() {
            20.
        } else {
            88.
        };
        let timed = self.notice_key().timed();
        let frozen_progress = self
            .notice
            .started_at
            .map(|start| (start.elapsed().as_secs_f32() / LIFETIME.as_secs_f32()).min(1.))
            .unwrap_or(0.);
        let text_color = if self.error { 0xa33929 } else { 0x245d43 };
        div()
            .absolute()
            .left(px(16.))
            .right(px(16.))
            .bottom(px(bottom))
            .flex()
            .justify_center()
            .child(
                overlays::panel("task-message")
                    .w(px(620.))
                    .max_w_full()
                    .max_h(px(144.))
                    .overflow_y_scroll()
                    .px_4()
                    .py_3()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(if self.error { 0xf0d5cf } else { 0xc8dfd1 }))
                    .bg(rgb(if self.error { 0xfff3f0 } else { TINT }))
                    .shadow_lg()
                    .text_color(rgb(text_color))
                    .text_sm()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().flex_1().min_w_0().child(self.message.clone()))
                            .when(!self.busy, |el| {
                                el.child(
                                    self.button(
                                        "dismiss-message",
                                        "",
                                        Command::DismissMessage,
                                        false,
                                        !leaving,
                                        cx,
                                    )
                                    .relative()
                                    .w(px(64.))
                                    .rounded(px(6.))
                                    .px_0()
                                    .overflow_hidden()
                                    .bg(rgb(if self.error { 0xffe5df } else { 0xe3efe7 }))
                                    .text_color(rgb(text_color))
                                    .when(timed, |el| {
                                        let fill = div()
                                            .absolute()
                                            .left_0()
                                            .top_0()
                                            .bottom_0()
                                            .rounded(px(6.));
                                        if leaving {
                                            // Preserve progress while the outgoing animation uses a new identity.
                                            el.child(
                                                fill.w(relative(frozen_progress)).bg(rgb(0x97c7a9)),
                                            )
                                        } else {
                                            el.child(fill.with_animation(
                                                ("notice-fill", generation),
                                                Animation::new(LIFETIME),
                                                |fill, progress| {
                                                    let channel = |start: f32, end: f32| {
                                                        (start + (end - start) * progress) as u32
                                                    };
                                                    let color = (channel(221., 151.) << 16)
                                                        | (channel(238., 199.) << 8)
                                                        | channel(226., 169.);
                                                    fill.w(relative(progress)).bg(rgb(color))
                                                },
                                            ))
                                        }
                                    })
                                    .child(
                                        div()
                                            .relative()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(overlays::close_icon(text_color)),
                                    ),
                                )
                            }),
                    ),
            )
            .with_animation(
                (if leaving { "notice-out" } else { "notice-in" }, generation),
                Animation::new(TRANSITION),
                move |toast, progress| {
                    let visibility = if leaving { 1. - progress } else { progress };
                    let eased = 1. - (1. - visibility).powi(3);
                    toast.opacity(eased).bottom(px(bottom - 8. * (1. - eased)))
                },
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_repeated_messages_invalidate_previous_timers() {
        let mut state = NoticeState::default();
        let first = NoticeKey {
            revision: 1,
            busy: false,
            error: false,
        };
        assert!(state.sync(first));
        let generation = state.generation;
        assert!(!state.sync(first));
        state.leaving = true;
        let repeated = NoticeKey {
            revision: 2,
            ..first
        };
        assert!(state.sync(repeated));
        assert!(!state.accepts(generation, first));
        assert!(!state.leaving);
    }

    #[test]
    fn running_and_failed_operations_never_auto_dismiss() {
        let success = NoticeKey {
            revision: 1,
            busy: false,
            error: false,
        };
        assert!(success.timed());
        assert!(
            !NoticeKey {
                busy: true,
                ..success
            }
            .timed()
        );
        assert!(
            !NoticeKey {
                error: true,
                ..success
            }
            .timed()
        );
        let mut state = NoticeState::default();
        state.sync(success);
        let generation = state.generation;
        let running = NoticeKey {
            busy: true,
            ..success
        };
        state.sync(running);
        assert!(!state.accepts(generation, success));
    }
}
