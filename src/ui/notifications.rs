//! The notification feed: small pop-ups about what happened ("Picked up: Rusted Axe",
//! "Guest 1 vigorated the Sun", …). Send a [`Notify`] message from anywhere.
//!
//! Each notification is an icon plus shadowed text with no background. It slides in from
//! outside the window at a screen corner. Newer ones go on top of the stack and push older ones
//! along `stack_step`, slightly shifted so they stay legible. Each lives `lifetime` seconds and
//! then fades. Everything is tunable in [`NotificationSettings`] (a resource: change it at
//! runtime, or its `Default` here).

use bevy::prelude::*;

use super::icons::{NoticeIcon, spawn_notice_icon};

pub struct NotificationsPlugin;

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NotificationSettings>()
            .add_message::<Notify>()
            .add_systems(Update, (spawn_notifications, animate_notifications).chain());
    }
}

/// A screen corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // all corners are valid settings
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// How notifications look and behave. All lengths in logical pixels, times in seconds.
#[derive(Resource, Clone, Debug)]
pub struct NotificationSettings {
    pub corner: Corner,
    /// Distance of the newest notification from the corner's edges (x from the side, y from the
    /// top/bottom).
    pub margin: Vec2,
    /// Fully visible this long…
    pub lifetime: f32,
    /// …then fades out over this long.
    pub fade: f32,
    /// Time to slide in from outside the window.
    pub slide_in: f32,
    /// Offset of each older notification from the one above it: `y` away from the edge (up for
    /// bottom corners), `x` inward from the side. Smaller than a row = they overlap a little.
    pub stack_step: Vec2,
    /// Older notifications are this much more transparent per step (1 = no dimming).
    pub older_alpha: f32,
    /// At most this many at once (the oldest beyond that disappear).
    pub max_shown: usize,
    pub font_size: f32,
    /// Icon size as a fraction of the 56 px item icons.
    pub icon_scale: f32,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            corner: Corner::BottomRight,
            margin: Vec2::new(28.0, 30.0),
            lifetime: 3.0,
            fade: 0.6,
            slide_in: 0.35,
            stack_step: Vec2::new(10.0, 28.0),
            older_alpha: 0.8,
            max_shown: 5,
            font_size: 18.0,
            icon_scale: 0.68,
        }
    }
}

/// Show a notification.
#[derive(Message, Debug, Clone)]
pub struct Notify {
    pub icon: NoticeIcon,
    pub text: String,
}

impl Notify {
    pub fn new(icon: NoticeIcon, text: impl Into<String>) -> Self {
        Self { icon, text: text.into() }
    }
}

#[derive(Component)]
struct Notice {
    age: f32,
    /// Order of arrival (newest = highest).
    seq: u64,
    /// Original colours of every node in the notice, to fade them together.
    colors: Vec<(Entity, Color, ColorKind)>,
}

#[derive(Clone, Copy)]
enum ColorKind {
    Background,
    Border,
    Text,
    Shadow,
}

/// Height of one notification row (for sliding in from just outside the window).
fn row_height(settings: &NotificationSettings) -> f32 {
    (56.0 * settings.icon_scale).max(settings.font_size * 1.4)
}

fn spawn_notifications(
    mut commands: Commands,
    mut messages: MessageReader<Notify>,
    settings: Res<NotificationSettings>,
    mut seq: Local<u64>,
) {
    for msg in messages.read() {
        *seq += 1;
        let right = matches!(settings.corner, Corner::TopRight | Corner::BottomRight);
        let row = commands
            .spawn((
                Name::new("Notification"),
                Node {
                    position_type: PositionType::Absolute,
                    // Icon on the outer side, text toward the screen centre.
                    flex_direction: if right { FlexDirection::RowReverse } else { FlexDirection::Row },
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                // Newer notifications are drawn over older ones.
                GlobalZIndex(8 + (*seq % 1000) as i32),
                Visibility::Hidden, // placed on the first animation frame
            ))
            .id();
        let icon_box = commands
            .spawn((
                Node {
                    width: Val::Px(56.0 * settings.icon_scale),
                    height: Val::Px(56.0 * settings.icon_scale),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                ChildOf(row),
            ))
            .id();
        spawn_notice_icon(&mut commands, icon_box, msg.icon, settings.icon_scale);
        commands.spawn((
            Text::new(msg.text.clone()),
            TextFont::from_font_size(settings.font_size),
            TextColor(Color::srgb(0.95, 0.92, 0.84)),
            // No background: a hard double shadow keeps it readable on any floor.
            TextShadow { offset: Vec2::splat(2.0), color: Color::srgba(0.0, 0.0, 0.0, 0.95) },
            ChildOf(row),
        ));
        commands.entity(row).insert(Notice { age: 0.0, seq: *seq, colors: vec![] });
    }
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[allow(clippy::type_complexity)]
fn animate_notifications(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<NotificationSettings>,
    mut notices: Query<(Entity, &mut Notice, &mut Node, &mut Visibility)>,
    children: Query<&Children>,
    mut colors: Query<(Option<&mut BackgroundColor>, Option<&mut BorderColor>, Option<&mut TextColor>, Option<&mut TextShadow>)>,
) {
    let dt = time.delta_secs();
    let mut order: Vec<(u64, Entity)> = notices.iter().map(|(e, n, _, _)| (n.seq, e)).collect();
    order.sort_by_key(|o| std::cmp::Reverse(o.0)); // newest first
    let s = &*settings;
    for (index, (_, entity)) in order.into_iter().enumerate() {
        let Ok((_, mut notice, mut node, mut vis)) = notices.get_mut(entity) else { continue };
        notice.age += dt;
        let gone = notice.age >= s.lifetime + s.fade || index >= s.max_shown;
        if gone {
            commands.entity(entity).despawn();
            continue;
        }
        // Remember every node's colour once (after the children exist).
        if notice.colors.is_empty() {
            let mut stack = vec![entity];
            while let Some(e) = stack.pop() {
                if let Ok((bg, border, text, shadow)) = colors.get(e) {
                    if let Some(sh) = shadow {
                        notice.colors.push((e, sh.color, ColorKind::Shadow));
                    }
                    if let Some(bg) = bg {
                        notice.colors.push((e, bg.0, ColorKind::Background));
                    }
                    if let Some(b) = border {
                        notice.colors.push((e, b.top, ColorKind::Border));
                    }
                    if let Some(t) = text {
                        notice.colors.push((e, t.0, ColorKind::Text));
                    }
                }
                if let Ok(c) = children.get(e) {
                    stack.extend(c.iter());
                }
            }
        }
        // Position: slide in from beyond the edge, then sit at its place in the stack.
        let slide = ease_out(notice.age / s.slide_in.max(1e-3));
        let off_screen = -(row_height(s) + s.margin.y + 8.0);
        let y = s.margin.y + s.stack_step.y * index as f32;
        let y = off_screen + (y - off_screen) * slide;
        let x = s.margin.x + s.stack_step.x * index as f32;
        let node = &mut *node;
        match s.corner {
            Corner::TopLeft | Corner::BottomLeft => node.left = Val::Px(x),
            Corner::TopRight | Corner::BottomRight => node.right = Val::Px(x),
        }
        match s.corner {
            Corner::TopLeft | Corner::TopRight => node.top = Val::Px(y),
            Corner::BottomLeft | Corner::BottomRight => node.bottom = Val::Px(y),
        }
        vis.set_if_neq(Visibility::Inherited);
        // Alpha: older ones dimmer; the end of the lifetime fades out.
        let fade = 1.0 - ((notice.age - s.lifetime) / s.fade.max(1e-3)).clamp(0.0, 1.0);
        let alpha = fade * s.older_alpha.powi(index as i32);
        for (e, base, kind) in &notice.colors {
            let Ok((bg, border, text, shadow)) = colors.get_mut(*e) else { continue };
            let c = base.with_alpha(base.alpha() * alpha);
            match kind {
                ColorKind::Background => {
                    if let Some(mut bg) = bg {
                        bg.0 = c;
                    }
                }
                ColorKind::Border => {
                    if let Some(mut b) = border {
                        b.set_all(c);
                    }
                }
                ColorKind::Text => {
                    if let Some(mut t) = text {
                        t.0 = c;
                    }
                }
                ColorKind::Shadow => {
                    if let Some(mut sh) = shadow {
                        sh.color = c;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slides_in_from_outside_and_settles() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert!(ease_out(0.5) > 0.5, "decelerates");
        let s = NotificationSettings::default();
        assert!(row_height(&s) > 20.0);
    }
}
