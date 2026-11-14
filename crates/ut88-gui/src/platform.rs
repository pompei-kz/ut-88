//! Окна ОС и цикл кадров: winit (окна и события) + softbuffer (вывод кадра из памяти).
//!
//! Здесь только обвязка: события winit переводятся в [`ui::Input`], кадр строит
//! [`App::frame`] (главное окно) или [`App::debugger_frame`] (окно отладчика) в своём
//! [`ui::Ctx`], готовый холст переводится в формат softbuffer и показывается. Кадры идут
//! по таймеру (`ControlFlow::WaitUntil`) с частотой монитора (по умолчанию 60 Гц): от этого
//! зависит ход эмуляции, поэтому окно перерисовывается непрерывно.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::App;
use crate::dialogs::RfdDialogs;
use crate::gfx::{pos2, vec2};
use crate::ui::{self, Ctx, CursorIcon, Input, Modifiers, PointerButton, SystemClipboard};

/// Точек прокрутки на одну строку колеса мыши.
const LINE_SCROLL: f32 = 40.0;
/// Частота кадров, если частота монитора неизвестна, Гц.
const DEFAULT_HZ: f64 = 60.0;
/// Пределы частоты кадров, Гц.
const MIN_HZ: f64 = 30.0;
const MAX_HZ: f64 = 240.0;

/// Окно ОС с поверхностью softbuffer и вводом.
struct Surf {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    /// Ввод, накопленный к следующему кадру.
    input: Input,
    /// Вид указателя, выставленный окну.
    cursor: CursorIcon,
    /// Начало прошлого кадра.
    last_frame: Option<Instant>,
}

impl Surf {
    /// Создать окно и его поверхность.
    fn open(event_loop: &ActiveEventLoop, attrs: WindowAttributes) -> anyhow::Result<Surf> {
        let window = Rc::new(
            event_loop
                .create_window(attrs)
                .map_err(|e| anyhow::anyhow!("окно не открылось: {e}"))?,
        );
        let context = softbuffer::Context::new(window.clone())
            .map_err(|e| anyhow::anyhow!("нет вывода кадра в окно: {e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("нет поверхности окна: {e}"))?;
        let mut s = Surf {
            window,
            surface,
            input: Input::default(),
            cursor: CursorIcon::Default,
            last_frame: None,
        };
        s.input.ppp = s.window.scale_factor() as f32;
        s.update_size();
        Ok(s)
    }

    /// Размер окна в точках — по размеру в пикселях и масштабу.
    fn update_size(&mut self) {
        let size = self.window.inner_size();
        let ppp = self.input.ppp.max(0.1);
        self.input.screen_size = vec2(size.width as f32 / ppp, size.height as f32 / ppp);
    }

    /// Перевести событие окна в ввод кадра.
    fn on_event(&mut self, event: &WindowEvent) {
        let ppp = self.input.ppp.max(0.1);
        let input = &mut self.input;
        match event {
            WindowEvent::Resized(_) => self.update_size(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                input.ppp = *scale_factor as f32;
                self.update_size();
            }
            WindowEvent::CursorMoved { position, .. } => {
                input.pointer = Some(pos2(position.x as f32 / ppp, position.y as f32 / ppp));
            }
            WindowEvent::CursorLeft { .. } => input.pointer = None,
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    _ => return,
                };
                input.pointer_button(b, *state == ElementState::Pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                input.scroll += match delta {
                    MouseScrollDelta::LineDelta(x, y) => vec2(*x, *y) * LINE_SCROLL,
                    MouseScrollDelta::PixelDelta(PhysicalPosition { x, y }) => {
                        vec2(*x as f32 / ppp, *y as f32 / ppp)
                    }
                };
            }
            WindowEvent::ModifiersChanged(m) => {
                let st = m.state();
                input.modifiers = Modifiers {
                    shift: st.shift_key(),
                    ctrl: st.control_key(),
                    alt: st.alt_key(),
                    logo: st.super_key(),
                };
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // Искусственные нажатия и отпускания при смене фокуса не нужны: потеря
                // фокуса и так отпускает все клавиши.
                if *is_synthetic {
                    return;
                }
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                let text = event.text.as_ref().map(|t| t.to_string());
                input.key(code, pressed, event.repeat, text.clone());
                let m = input.modifiers;
                if pressed
                    && !(m.ctrl || m.alt || m.logo)
                    && let Some(t) = text
                {
                    input.text(&t);
                }
            }
            WindowEvent::Focused(on) => {
                input.focused = *on;
                input.events.push(ui::Event::WindowFocused(*on));
                if !on {
                    input.modifiers = Modifiers::NONE;
                    for b in [
                        PointerButton::Primary,
                        PointerButton::Secondary,
                        PointerButton::Middle,
                    ] {
                        if input.down[b as usize] {
                            input.pointer_button(b, false);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Кадр: построить его функцией `build` в контексте `ctx` и показать в окне.
    fn frame(&mut self, ctx: &Ctx, build: impl FnOnce(&Ctx)) -> anyhow::Result<()> {
        let now = Instant::now();
        if let Some(t) = self.last_frame {
            self.input.dt = now.duration_since(t).as_secs_f32();
        }
        self.last_frame = Some(now);
        self.input.time += f64::from(self.input.dt);
        ctx.begin_frame(&self.input);
        build(ctx);
        let out = ctx.end_frame();
        self.input.next_frame();

        if out.cursor_icon != self.cursor {
            self.cursor = out.cursor_icon;
            self.window.set_cursor(match out.cursor_icon {
                CursorIcon::Default => winit::window::CursorIcon::Default,
                CursorIcon::PointingHand => winit::window::CursorIcon::Pointer,
                CursorIcon::Text => winit::window::CursorIcon::Text,
                CursorIcon::Grabbing => winit::window::CursorIcon::Grabbing,
            });
        }

        // Свёрнутое окно: кадр построен (эмуляция идёт), но показывать нечего.
        let PhysicalSize { width, height } = self.window.inner_size();
        if width == 0 || height == 0 {
            return Ok(());
        }
        let (w, h) = ctx.with_canvas(|c| (c.width(), c.height()));
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return Ok(());
        };
        self.surface
            .resize(nw, nh)
            .map_err(|e| anyhow::anyhow!("размер поверхности окна: {e}"))?;
        let mut buf = self
            .surface
            .buffer_mut()
            .map_err(|e| anyhow::anyhow!("буфер кадра: {e}"))?;
        ctx.with_canvas(|c| c.to_0rgb(&mut buf));
        self.window.pre_present_notify();
        buf.present()
            .map_err(|e| anyhow::anyhow!("вывод кадра: {e}"))?;
        Ok(())
    }
}

/// Обработчик событий: главное окно, окно отладчика и такт кадров.
struct Platform {
    app: App,
    full: bool,
    main: Option<Surf>,
    main_ctx: Ctx,
    dbg: Option<Surf>,
    /// Контекст окна отладчика живёт дольше самого окна: прокрутка и поля сохраняются
    /// между открытиями.
    dbg_ctx: Ctx,
    /// Время следующего кадра.
    next_frame: Instant,
    /// Период кадров.
    period: Duration,
    /// Ошибка, из-за которой цикл остановлен.
    error: Option<anyhow::Error>,
    /// Выход уже начат (квазидиск сохранён).
    exited: bool,
}

impl Platform {
    fn new(app: App, full: bool) -> Self {
        let main_ctx = Ctx::new();
        main_ctx.set_clipboard(Box::new(SystemClipboard::new()));
        let dbg_ctx = Ctx::new();
        dbg_ctx.set_clipboard(Box::new(SystemClipboard::new()));
        Platform {
            app,
            full,
            main: None,
            main_ctx,
            dbg: None,
            dbg_ctx,
            next_frame: Instant::now(),
            period: Duration::from_secs_f64(1.0 / DEFAULT_HZ),
            error: None,
            exited: false,
        }
    }

    /// Остановиться с ошибкой.
    fn fail(&mut self, event_loop: &ActiveEventLoop, e: anyhow::Error) {
        self.error.get_or_insert(e);
        event_loop.exit();
    }

    /// Выход: сохранить квазидиск и закончить цикл.
    fn quit(&mut self, event_loop: &ActiveEventLoop) {
        if !std::mem::replace(&mut self.exited, true) {
            self.app.on_exit();
        }
        self.dbg = None;
        self.main = None;
        event_loop.exit();
    }

    /// Открыть или закрыть окно отладчика по состоянию приложения.
    fn sync_debugger(&mut self, event_loop: &ActiveEventLoop) {
        match (self.app.debugger_open(), self.dbg.is_some()) {
            (true, false) => {
                let attrs = Window::default_attributes()
                    .with_title("ЮТ-88 — отладчик")
                    .with_inner_size(LogicalSize::new(820.0, 760.0))
                    .with_min_inner_size(LogicalSize::new(400.0, 300.0));
                match Surf::open(event_loop, attrs) {
                    Ok(s) => self.dbg = Some(s),
                    Err(e) => {
                        eprintln!("ut88: {e:#}");
                        self.app.close_debugger();
                    }
                }
            }
            // Окно уничтожается; контекст остаётся.
            (false, true) => self.dbg = None,
            _ => {}
        }
    }

    /// Частота кадров по монитору главного окна.
    fn update_period(&mut self) {
        let hz = self
            .main
            .as_ref()
            .and_then(|s| s.window.current_monitor())
            .and_then(|m| m.refresh_rate_millihertz())
            .map(|mhz| f64::from(mhz) / 1000.0)
            .filter(|hz| hz.is_finite() && *hz > 0.0)
            .unwrap_or(DEFAULT_HZ)
            .clamp(MIN_HZ, MAX_HZ);
        self.period = Duration::from_secs_f64(1.0 / hz);
    }

    /// Такт: кадр главного окна (с эмуляцией), затем окна отладчика.
    fn tick(&mut self, event_loop: &ActiveEventLoop) {
        let Some(main) = self.main.as_mut() else {
            return;
        };
        let app = &mut self.app;
        if let Err(e) = main.frame(&self.main_ctx, |ctx| app.frame(ctx)) {
            self.fail(event_loop, e);
            return;
        }
        if self.app.quit_requested() {
            self.quit(event_loop);
            return;
        }
        self.sync_debugger(event_loop);
        if let Some(dbg) = self.dbg.as_mut() {
            let app = &mut self.app;
            if let Err(e) = dbg.frame(&self.dbg_ctx, |ctx| app.debugger_frame(ctx)) {
                // Отладчик не главное: закрыть его окно и работать дальше.
                eprintln!("ut88: окно отладчика: {e:#}");
                self.app.close_debugger();
                self.dbg = None;
            }
        }
    }
}

impl ApplicationHandler for Platform {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.main.is_some() {
            return;
        }
        let size = if self.full {
            LogicalSize::new(1280.0, 900.0)
        } else {
            LogicalSize::new(720.0, 680.0)
        };
        let attrs = Window::default_attributes()
            .with_title("ЮТ-88")
            .with_inner_size(size)
            .with_min_inner_size(LogicalSize::new(480.0, 400.0));
        match Surf::open(event_loop, attrs) {
            Ok(s) => {
                self.app
                    .set_file_dialogs(Box::new(RfdDialogs::with_parent(s.window.clone())));
                self.main = Some(s);
                self.update_period();
                self.next_frame = Instant::now();
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
            }
            Err(e) => self.fail(event_loop, e),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let is_main = self.main.as_ref().is_some_and(|s| s.window.id() == id);
        let is_dbg = self.dbg.as_ref().is_some_and(|s| s.window.id() == id);
        if matches!(event, WindowEvent::CloseRequested) {
            if is_main {
                self.quit(event_loop);
            } else if is_dbg {
                self.app.close_debugger();
                self.dbg = None;
            }
            return;
        }
        let surf = if is_main {
            self.main.as_mut()
        } else if is_dbg {
            self.dbg.as_mut()
        } else {
            None
        };
        let Some(surf) = surf else { return };
        surf.on_event(&event);
        match event {
            // Перерисовать как можно скорее (кадр строится в about_to_wait).
            WindowEvent::RedrawRequested | WindowEvent::Resized(_) => {
                self.next_frame = self.next_frame.min(Instant::now());
            }
            WindowEvent::ScaleFactorChanged { .. } if is_main => self.update_period(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_frame {
            self.tick(event_loop);
            if event_loop.exiting() {
                return;
            }
            self.next_frame += self.period;
            if self.next_frame <= now {
                // Не успели — не догонять пропущенные кадры.
                self.next_frame = now + self.period;
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}

/// Открыть главное окно с приложением `app` и работать до выхода.
pub fn run(app: App, full: bool) -> anyhow::Result<()> {
    let event_loop = EventLoop::new().map_err(|e| anyhow::anyhow!("окно не открылось: {e}"))?;
    let mut p = Platform::new(app, full);
    event_loop
        .run_app(&mut p)
        .map_err(|e| anyhow::anyhow!("окно: {e}"))?;
    match p.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
