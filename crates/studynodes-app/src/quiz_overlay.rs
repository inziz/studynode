use rune::{Color, Cursor, Cx, El, Icon, ScrollSpec, TextStyle, col, el, row, text};
use studynodes_graph::model::{NodeKind, QuizDifficulty};

use crate::icons;
use crate::state::{AppState, QuizAttemptStatus};
use crate::theme;

fn topbar_icon_btn(
    key: &'static str,
    ico: &'static Icon,
    fg: Color,
    fill: Color,
    hover: Color,
    border: Color,
    action: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key)
        .size(26.0, 26.0)
        .radius(6.0)
        .fill(fill)
        .hover(hover)
        .press(theme::press_subtle())
        .border(1.0, border)
        .center()
        .cursor(Cursor::Pointer)
        .no_window_drag()
        .on_press(action)
        .child(icons::ph(ico, 14.0, fg))
}

fn card_icon_btn(
    key: &'static str,
    ico: &'static Icon,
    fg: Color,
    fill: Color,
    hover: Color,
    border: Color,
    enabled: bool,
    action: impl Fn(&mut AppState) + 'static,
) -> El {
    el()
        .key(key)
        .size(30.0, 30.0)
        .radius(6.0)
        .fill(fill)
        .hover(if enabled { hover } else { fill })
        .press(if enabled {
            theme::press_subtle()
        } else {
            fill
        })
        .border(1.0, border)
        .center()
        .cursor(if enabled {
            Cursor::Pointer
        } else {
            Cursor::Default
        })
        .on_press(move |s: &mut AppState| {
            if enabled {
                action(s);
            }
        })
        .child(icons::ph(ico, 15.0, fg))
}

pub fn render_quiz_popout_overlay(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let insets = cx.chrome_insets();
    let accent = theme::kind_accent(NodeKind::Quiz);
    let danger = theme::accent_danger();
    let ok_col = theme::hex("#6AB07A");

    let Some(node) = s
        .active_quiz_overlay
        .as_ref()
        .and_then(|id| s.graph.get_node(id))
        .filter(|n| n.kind == NodeKind::Quiz)
    else {
        return render_unavailable_overlay(insets.leading, insets.trailing);
    };

    let questions = &node.meta.quiz_questions;
    let total = questions.len();
    if total == 0 {
        return render_unavailable_overlay(insets.leading, insets.trailing);
    }

    let is_hard = node.meta.quiz_difficulty == QuizDifficulty::Hard;
    let hard_locked = s.is_quiz_hard_locked();
    let sess = s.quiz_sessions.get(&node.id);
    let status = sess.map(|st| st.status).unwrap_or(QuizAttemptStatus::InProgress);
    let q_idx = sess.map(|st| st.active_index).unwrap_or(0).min(total - 1);

    let answered_count = (0..total)
        .filter(|qi| sess.is_some_and(|st| st.selected_options.contains_key(qi)))
        .count();

    let (graded_total, graded_correct) =
        questions
            .iter()
            .enumerate()
            .fold((0usize, 0usize), |(tot, cor), (qi, q)| {
                if let Some(ci) = q.correct_index {
                    let hit = sess.and_then(|st| st.selected_options.get(&qi).copied()) == Some(ci);
                    (tot + 1, cor + usize::from(hit))
                } else {
                    (tot, cor)
                }
            });

    // App-consistent 36px chrome titlebar with proper trailing inset for window controls
    let mut topbar = row()
        .key("quiz_overlay_topbar")
        .w_full()
        .h(36.0)
        .pl(insets.leading + 8.0)
        .pr(insets.trailing + 12.0)
        .gap(8.0)
        .center_y()
        .fill(theme::bg_canvas())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .window_drag();

    if !hard_locked {
        topbar = topbar.child(topbar_icon_btn(
            "btn_quiz_overlay_back",
            &icons::ARROW_LEFT,
            theme::text_secondary(),
            Color::TRANSPARENT,
            theme::hover_subtle(),
            Color::TRANSPARENT,
            |s: &mut AppState| {
                s.quiz_return_to_scheme();
            },
        ));
    }

    topbar = topbar
        .child(icons::ph(&icons::QUESTION, 15.0, accent))
        .child(
            text(&node.title)
                .font(theme::PANEL_TITLE)
                .color(theme::text_primary()),
        )
        .child(el().grow(1.0));

    if matches!(
        status,
        QuizAttemptStatus::NotStarted | QuizAttemptStatus::InProgress
    ) {
        topbar = topbar.child(
            text(format!("{}/{}", q_idx + 1, total))
                .font(theme::STATUS_TEXT)
                .color(theme::text_muted()),
        );
    }

    if !is_hard {
        let reset_id = node.id.clone();
        topbar = topbar.child(topbar_icon_btn(
            "btn_quiz_overlay_reset",
            &icons::ARROW_CLOCKWISE,
            theme::text_secondary(),
            Color::TRANSPARENT,
            theme::hover_subtle(),
            Color::TRANSPARENT,
            move |s: &mut AppState| {
                s.quiz_retry_popout(&reset_id);
            },
        ));
    } else if hard_locked {
        topbar = topbar.child(topbar_icon_btn(
            "btn_quiz_overlay_give_up",
            &icons::X,
            danger,
            danger.alpha(0.12),
            danger.alpha(0.22),
            danger.alpha(0.40),
            |s: &mut AppState| {
                s.quiz_give_up_popout();
            },
        ));
    }

    let body_inner: El = match status {
        QuizAttemptStatus::Failed => {
            let retry_id = node.id.clone();
            el()
                .w_full()
                .max_w(480.0)
                .px(24.0)
                .py(24.0)
                .radius(8.0)
                .fill(theme::bg_card())
                .border(1.0, danger.alpha(0.40))
                .child(
                    col()
                        .w_full()
                        .gap(12.0)
                        .child(
                            row()
                                .items_center()
                                .gap(8.0)
                                .child(icons::ph(&icons::X, 16.0, danger))
                                .child(
                                    text(&node.title)
                                        .font(TextStyle::new(16.0, 600.0).lh(22.0))
                                        .color(theme::text_primary()),
                                ),
                        )
                        .child(
                            text(format!("Failed · {}/{}", answered_count, total))
                                .font(theme::STATUS_TEXT)
                                .color(danger),
                        )
                        .child(
                            row()
                                .w_full()
                                .pt(6.0)
                                .gap(8.0)
                                .justify_between()
                                .child(card_icon_btn(
                                    "btn_quiz_overlay_done_back",
                                    &icons::ARROW_LEFT,
                                    theme::text_secondary(),
                                    theme::bg_control(),
                                    theme::hover_subtle(),
                                    theme::border_subtle(),
                                    true,
                                    |s: &mut AppState| {
                                        s.quiz_return_to_scheme();
                                    },
                                ))
                                .child(card_icon_btn(
                                    "btn_quiz_overlay_try_again",
                                    &icons::ARROW_CLOCKWISE,
                                    danger,
                                    danger.alpha(0.14),
                                    danger.alpha(0.24),
                                    danger.alpha(0.45),
                                    true,
                                    move |s: &mut AppState| {
                                        s.quiz_retry_popout(&retry_id);
                                    },
                                )),
                        ),
                )
        }
        QuizAttemptStatus::Completed => {
            let retry_id = node.id.clone();
            let headline = if graded_total > 0 {
                let pct = (graded_correct * 100) / graded_total;
                format!("{} / {} ({}%)", graded_correct, graded_total, pct)
            } else {
                format!("{} / {}", total, total)
            };

            let mut review_col = col().w_full().gap(8.0);
            for (qi, qq) in questions.iter().enumerate() {
                let picked = sess.and_then(|st| st.selected_options.get(&qi).copied());
                let (badge_ico, badge_col) = match (qq.correct_index, picked) {
                    (Some(ci), Some(pi)) if ci == pi => (&icons::CHECK, ok_col),
                    (Some(_), Some(_)) => (&icons::X, danger),
                    _ => (&icons::QUESTION, theme::text_muted()),
                };

                let rev_title = if qq.question.trim().is_empty() {
                    format!("Q{}", qi + 1)
                } else {
                    format!("{}. {}", qi + 1, qq.question)
                };
                let mut item = col()
                    .w_full()
                    .px(12.0)
                    .py(10.0)
                    .radius(6.0)
                    .fill(theme::bg_input())
                    .border(1.0, theme::border_subtle())
                    .gap(5.0)
                    .child(
                        row()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap(8.0)
                            .child(
                                text(rev_title)
                                    .font(theme::BODY_TEXT)
                                    .color(theme::text_primary())
                                    .wrap(),
                            )
                            .child(icons::ph(badge_ico, 14.0, badge_col)),
                    );

                if let Some(q_img) = s.quiz_image_for_hash(qq.question_image_hash.as_deref()) {
                    let h = theme::scaled_image_height(q_img, 580.0, 56.0, 120.0, 84.0);
                    item = item.child(theme::render_fitted_image_box(
                        format!("quiz_overlay_rev_q_img_{}", qi),
                        q_img,
                        h,
                        5.0,
                    ));
                }
                if let Some(pi) = picked {
                    if let Some(opt_txt) = qq.options.get(pi) {
                        item = item.child(
                            text(opt_txt)
                                .font(theme::STATUS_TEXT)
                                .color(theme::text_secondary())
                                .wrap(),
                        );
                    }
                }
                if let Some(ci) = qq.correct_index {
                    if picked != Some(ci) {
                        if let Some(corr_txt) = qq.options.get(ci) {
                            item = item.child(
                                text(corr_txt)
                                    .font(theme::STATUS_TEXT)
                                    .color(ok_col)
                                    .wrap(),
                            );
                        }
                    }
                }
                if !qq.answer.trim().is_empty() {
                    item = item.child(
                        text(qq.answer.trim())
                            .font(theme::STATUS_TEXT)
                            .color(theme::text_muted())
                            .wrap(),
                    );
                }
                if let Some(a_img) = s.quiz_image_for_hash(qq.answer_image_hash.as_deref()) {
                    let h = theme::scaled_image_height(a_img, 580.0, 56.0, 120.0, 84.0);
                    item = item.child(theme::render_fitted_image_box(
                        format!("quiz_overlay_rev_ans_img_{}", qi),
                        a_img,
                        h,
                        5.0,
                    ));
                }
                review_col = review_col.child(item);
            }

            el()
                .w_full()
                .max_w(640.0)
                .px(22.0)
                .py(22.0)
                .radius(8.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .child(
                    col()
                        .w_full()
                        .gap(14.0)
                        .child(
                            row()
                                .items_center()
                                .gap(8.0)
                                .child(icons::ph(&icons::CHECK, 16.0, ok_col))
                                .child(
                                    text(headline)
                                        .font(TextStyle::new(17.0, 600.0).lh(24.0))
                                        .color(theme::text_primary()),
                                ),
                        )
                        .child(review_col)
                        .child(
                            row()
                                .w_full()
                                .pt(6.0)
                                .gap(8.0)
                                .justify_between()
                                .child(card_icon_btn(
                                    "btn_quiz_overlay_done_back",
                                    &icons::ARROW_LEFT,
                                    theme::text_secondary(),
                                    theme::bg_control(),
                                    theme::hover_subtle(),
                                    theme::border_subtle(),
                                    true,
                                    |s: &mut AppState| {
                                        s.quiz_return_to_scheme();
                                    },
                                ))
                                .child(card_icon_btn(
                                    "btn_quiz_overlay_try_again",
                                    &icons::ARROW_CLOCKWISE,
                                    accent,
                                    accent.alpha(0.14),
                                    accent.alpha(0.24),
                                    accent.alpha(0.45),
                                    true,
                                    move |s: &mut AppState| {
                                        s.quiz_retry_popout(&retry_id);
                                    },
                                )),
                        ),
                )
        }
        QuizAttemptStatus::NotStarted | QuizAttemptStatus::InProgress => {
            let q = &questions[q_idx];
            let selected_opt = sess.and_then(|st| st.selected_options.get(&q_idx).copied());
            let revealed_hints = sess
                .and_then(|st| st.revealed_hints.get(&q_idx).copied())
                .unwrap_or(0)
                .min(q.hints.len());
            let answer_shown = sess.is_some_and(|st| st.revealed_answers.contains(&q_idx));

            let mut step_row = row().w_full().gap(6.0).scroll(ScrollSpec { stick_bottom: false });
            for i in 0..total {
                let step_id = node.id.clone();
                let is_cur = i == q_idx;
                let has_sel = sess.is_some_and(|st| st.selected_options.contains_key(&i));
                step_row = step_row.child(
                    el()
                        .key(format!("quiz_overlay_step_{}", i))
                        .px(9.0)
                        .py(4.0)
                        .radius(5.0)
                        .fill(if is_cur {
                            accent.alpha(0.18)
                        } else if has_sel {
                            theme::bg_card()
                        } else {
                            Color::TRANSPARENT
                        })
                        .hover(theme::hover_subtle())
                        .border(
                            1.0,
                            if is_cur {
                                accent.alpha(0.75)
                            } else if has_sel {
                                ok_col.alpha(0.50)
                            } else {
                                theme::border_subtle()
                            },
                        )
                        .cursor(Cursor::Pointer)
                        .on_press(move |s: &mut AppState| {
                            s.quiz_jump_question(&step_id, i);
                        })
                        .child(
                            text(format!("{}", i + 1))
                                .font(theme::BADGE_TEXT)
                                .color(if is_cur {
                                    accent
                                } else {
                                    theme::text_secondary()
                                }),
                        ),
                );
            }

            let mut card = col()
                .w_full()
                .px(22.0)
                .py(20.0)
                .radius(8.0)
                .fill(theme::bg_card())
                .border(1.0, theme::border_subtle())
                .gap(14.0);
            if !q.question.trim().is_empty() {
                card = card.child(
                    text(&q.question)
                        .font(TextStyle::new(16.0, 600.0).lh(24.0))
                        .color(theme::text_primary())
                        .wrap(),
                );
            }
            if let Some(q_img) = s.quiz_image_for_hash(q.question_image_hash.as_deref()) {
                let q_img_h = theme::scaled_image_height(q_img, 636.0, 110.0, 300.0, 190.0);
                card = card.child(theme::render_fitted_image_box(
                    "quiz_overlay_q_img",
                    q_img,
                    q_img_h,
                    6.0,
                ));
            }

            if !q.options.is_empty() {
                let mut opts_col = col().w_full().gap(8.0);
                for (oi, opt) in q.options.iter().enumerate() {
                    let opt_id = node.id.clone();
                    let is_sel = selected_opt == Some(oi);
                    let is_correct = q.correct_index == Some(oi);
                    let letter = char::from(b'A' + (oi as u8).min(25));

                    let (fill_c, border_c, fg_c) = if answer_shown {
                        if is_correct {
                            (ok_col.alpha(0.14), ok_col.alpha(0.70), ok_col)
                        } else if is_sel {
                            (danger.alpha(0.14), danger.alpha(0.70), danger)
                        } else {
                            (theme::bg_input(), theme::border_subtle(), theme::text_primary())
                        }
                    } else if is_sel {
                        (accent.alpha(0.16), accent.alpha(0.80), accent)
                    } else {
                        (theme::bg_input(), theme::border_subtle(), theme::text_primary())
                    };

                    opts_col = opts_col.child(
                        el()
                            .key(format!("quiz_overlay_opt_{}", oi))
                            .w_full()
                            .px(12.0)
                            .py(9.0)
                            .radius(6.0)
                            .fill(fill_c)
                            .hover(theme::hover_subtle())
                            .border(1.0, border_c)
                            .cursor(Cursor::Pointer)
                            .on_press(move |s: &mut AppState| {
                                s.quiz_select_option(&opt_id, q_idx, oi);
                            })
                            .child(
                                row()
                                    .w_full()
                                    .items_center()
                                    .gap(10.0)
                                    .child(
                                        el()
                                            .px(6.0)
                                            .py(2.5)
                                            .radius(4.0)
                                            .fill(if is_sel || (answer_shown && is_correct) {
                                                fg_c.alpha(0.18)
                                            } else {
                                                theme::bg_control()
                                            })
                                            .child(
                                                text(format!("{}", letter))
                                                    .font(theme::BADGE_TEXT)
                                                    .color(
                                                        if is_sel || (answer_shown && is_correct) {
                                                            fg_c
                                                        } else {
                                                            theme::text_secondary()
                                                        },
                                                    ),
                                            ),
                                    )
                                    .child(
                                        text(opt)
                                            .font(theme::BODY_TEXT)
                                            .color(theme::text_primary())
                                            .wrap(),
                                    ),
                            ),
                    );
                }
                card = card.child(opts_col);
            }

            if revealed_hints > 0 {
                let mut hints_col = col().w_full().gap(6.0);
                for htxt in q.hints.iter().take(revealed_hints) {
                    hints_col = hints_col.child(
                        row()
                            .w_full()
                            .px(11.0)
                            .py(8.0)
                            .radius(6.0)
                            .fill(accent.alpha(0.08))
                            .border(1.0, accent.alpha(0.28))
                            .items_center()
                            .gap(8.0)
                            .child(icons::ph(&icons::LIGHTBULB, 13.0, accent))
                            .child(
                                text(htxt)
                                    .font(theme::BODY_TEXT)
                                    .color(theme::text_secondary())
                                    .wrap(),
                            ),
                    );
                }
                card = card.child(hints_col);
            }

            if answer_shown {
                let corr_line = q.correct_index.and_then(|ci| q.options.get(ci)).map(|txt| {
                    let letter = char::from(b'A' + (q.correct_index.unwrap_or(0) as u8).min(25));
                    format!("{} — {}", letter, txt)
                });
                let ans_img = s.quiz_image_for_hash(q.answer_image_hash.as_deref());
                if corr_line.is_some() || !q.answer.trim().is_empty() || ans_img.is_some() {
                    let mut ans_box = col()
                        .w_full()
                        .px(11.0)
                        .py(9.0)
                        .radius(6.0)
                        .fill(ok_col.alpha(0.10))
                        .border(1.0, ok_col.alpha(0.35))
                        .gap(6.0);
                    if let Some(line) = corr_line {
                        ans_box = ans_box.child(text(line).font(theme::CONTROL_TEXT).color(ok_col).wrap());
                    }
                    if !q.answer.trim().is_empty() {
                        ans_box = ans_box.child(
                            text(q.answer.trim())
                                .font(theme::BODY_TEXT)
                                .color(theme::text_primary())
                                .wrap(),
                        );
                    }
                    if let Some(a_img) = ans_img {
                        let a_img_h = theme::scaled_image_height(a_img, 610.0, 96.0, 240.0, 160.0);
                        ans_box = ans_box.child(theme::render_fitted_image_box(
                            "quiz_overlay_ans_img",
                            a_img,
                            a_img_h,
                            6.0,
                        ));
                    }
                    card = card.child(ans_box);
                }
            }

            // Footer: icon-only assist buttons on left, compact progress center, icon-only nav on right
            let mut assist_row = row().items_center().gap(6.0);
            if !q.hints.is_empty() {
                let hint_id = node.id.clone();
                let more_hints = revealed_hints < q.hints.len();
                assist_row = assist_row.child(card_icon_btn(
                    "quiz_overlay_hint_btn",
                    &icons::LIGHTBULB,
                    if revealed_hints > 0 {
                        accent
                    } else {
                        theme::text_secondary()
                    },
                    if revealed_hints > 0 {
                        accent.alpha(0.14)
                    } else {
                        Color::TRANSPARENT
                    },
                    theme::hover_subtle(),
                    theme::border_subtle(),
                    more_hints,
                    move |s: &mut AppState| {
                        s.quiz_reveal_next_hint(&hint_id, q_idx);
                    },
                ));
            }

            if !q.answer.trim().is_empty()
                || q.correct_index.is_some()
                || q.answer_image_hash.is_some()
            {
                let ans_id = node.id.clone();
                assist_row = assist_row.child(card_icon_btn(
                    "quiz_overlay_ans_btn",
                    &icons::CHECK_SQUARE,
                    if answer_shown {
                        ok_col
                    } else {
                        theme::text_secondary()
                    },
                    if answer_shown {
                        ok_col.alpha(0.14)
                    } else {
                        Color::TRANSPARENT
                    },
                    theme::hover_subtle(),
                    theme::border_subtle(),
                    true,
                    move |s: &mut AppState| {
                        s.quiz_reveal_answer(&ans_id, q_idx);
                    },
                ));
            }

            let prev_id = node.id.clone();
            let can_prev = q_idx > 0;
            let prev_btn = card_icon_btn(
                "btn_quiz_overlay_prev",
                &icons::CARET_LEFT,
                if can_prev {
                    theme::text_primary()
                } else {
                    theme::text_muted()
                },
                Color::TRANSPARENT,
                theme::hover_subtle(),
                theme::border_subtle(),
                can_prev,
                move |s: &mut AppState| {
                    s.quiz_step_question(&prev_id, -1);
                },
            );

            let right_nav = if q_idx + 1 < total {
                let next_id = node.id.clone();
                card_icon_btn(
                    "btn_quiz_overlay_next",
                    &icons::CARET_RIGHT,
                    accent,
                    accent.alpha(0.14),
                    accent.alpha(0.24),
                    accent.alpha(0.45),
                    true,
                    move |s: &mut AppState| {
                        s.quiz_step_question(&next_id, 1);
                    },
                )
            } else {
                card_icon_btn(
                    "btn_quiz_overlay_finish",
                    &icons::CHECK,
                    ok_col,
                    ok_col.alpha(0.16),
                    ok_col.alpha(0.26),
                    ok_col.alpha(0.50),
                    true,
                    |s: &mut AppState| {
                        s.quiz_finish_popout();
                    },
                )
            };

            let footer = row()
                .w_full()
                .items_center()
                .justify_between()
                .child(assist_row)
                .child(
                    text(format!("{}/{}", answered_count, total))
                        .font(theme::STATUS_TEXT)
                        .color(theme::text_muted()),
                )
                .child(row().items_center().gap(6.0).child(prev_btn).child(right_nav));

            el().w_full().max_w(680.0).child(
                col()
                    .w_full()
                    .gap(12.0)
                    .child(step_row)
                    .child(card)
                    .child(footer),
            )
        }
    };

    col()
        .key("quiz_popout_overlay")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(theme::bg_canvas())
        .child(topbar)
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec { stick_bottom: false })
                .px(28.0)
                .py(28.0)
                .items_center()
                .child(body_inner),
        )
}

fn render_unavailable_overlay(leading_inset: f32, trailing_inset: f32) -> El {
    let topbar = row()
        .key("quiz_overlay_topbar")
        .w_full()
        .h(36.0)
        .pl(leading_inset + 8.0)
        .pr(trailing_inset + 12.0)
        .gap(8.0)
        .center_y()
        .fill(theme::bg_canvas())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::border_subtle())
        .window_drag()
        .child(topbar_icon_btn(
            "btn_quiz_overlay_back",
            &icons::ARROW_LEFT,
            theme::text_secondary(),
            Color::TRANSPARENT,
            theme::hover_subtle(),
            Color::TRANSPARENT,
            |s: &mut AppState| {
                s.quiz_return_to_scheme();
            },
        ));

    col()
        .key("quiz_popout_overlay")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(theme::bg_canvas())
        .child(topbar)
        .child(
            col()
                .w_full()
                .grow(1.0)
                .center()
                .child(
                    text("No questions")
                        .font(theme::BODY_TEXT)
                        .color(theme::text_muted()),
                ),
        )
}
