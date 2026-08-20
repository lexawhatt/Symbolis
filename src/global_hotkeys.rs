use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver},
    thread,
};

use eframe::egui::Context;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{HotKey, Modifiers},
};

use crate::settings::{HotkeyAction, HotkeyBinding, HotkeySettings};

pub(crate) struct GlobalHotkeyRuntime {
    manager: Option<GlobalHotKeyManager>,
    registered: Vec<HotKey>,
    actions_by_id: HashMap<u32, HotkeyAction>,
    event_rx: Receiver<GlobalHotKeyEvent>,
    status: String,
}

impl GlobalHotkeyRuntime {
    pub(crate) fn new(settings: &HotkeySettings, ctx: Context) -> Self {
        let event_rx = spawn_hotkey_event_forwarder(ctx);
        let mut runtime = Self {
            manager: None,
            registered: Vec::new(),
            actions_by_id: HashMap::new(),
            event_rx,
            status: String::new(),
        };
        runtime.rebuild(settings);
        runtime
    }

    pub(crate) fn rebuild(&mut self, settings: &HotkeySettings) {
        self.unregister_all();
        self.actions_by_id.clear();

        if !settings.enabled {
            self.status = "Built-in hotkey backend disabled in Preferences".to_owned();
            return;
        }

        if let Err(err) = self.ensure_manager() {
            self.status = format!("Built-in hotkeys unavailable: {err}");
            return;
        };
        let Some(manager) = &self.manager else {
            self.status = "Built-in hotkeys unavailable".to_owned();
            return;
        };

        let mut errors = Vec::new();
        for action in HotkeyAction::CHOICES {
            let Some(binding) = settings.binding(action) else {
                continue;
            };
            if binding.key.trim().is_empty() {
                continue;
            }
            match parse_binding(binding) {
                Ok(hotkey) => {
                    let id = hotkey.id();
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        self.actions_by_id.entry(id)
                    {
                        match manager.register(hotkey) {
                            Ok(()) => {
                                entry.insert(action);
                                self.registered.push(hotkey);
                            }
                            Err(err) => {
                                errors.push(format!("{}: {err}", action.label()));
                            }
                        }
                    } else {
                        errors.push(format!("{} duplicates another hotkey", action.label()));
                    }
                }
                Err(err) => errors.push(format!("{}: {err}", action.label())),
            }
        }

        self.status = if errors.is_empty() {
            if self.registered.is_empty() {
                "No global hotkey set".to_owned()
            } else {
                format!(
                    "Global hotkey active: {}",
                    self.registered
                        .iter()
                        .map(|hotkey| hotkey.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        } else {
            format!("Global hotkey warning: {}", errors.join("; "))
        };
    }

    pub(crate) fn poll(&self) -> Vec<HotkeyAction> {
        let mut actions = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if let Some(action) = self.actions_by_id.get(&event.id).copied() {
                actions.push(action);
            }
        }
        actions
    }

    pub(crate) fn status(&self) -> &str {
        &self.status
    }

    fn unregister_all(&mut self) {
        let Some(manager) = &self.manager else {
            self.registered.clear();
            return;
        };

        for hotkey in self.registered.drain(..) {
            let _ = manager.unregister(hotkey);
        }
    }

    fn ensure_manager(&mut self) -> Result<(), String> {
        if self.manager.is_some() {
            return Ok(());
        }
        GlobalHotKeyManager::new()
            .map(|manager| {
                self.manager = Some(manager);
            })
            .map_err(|err| err.to_string())
    }
}

fn spawn_hotkey_event_forwarder(ctx: Context) -> Receiver<GlobalHotKeyEvent> {
    let (tx, rx) = mpsc::channel();
    let receiver = GlobalHotKeyEvent::receiver().clone();
    thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            if tx.send(event).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
    rx
}

fn parse_binding(binding: &HotkeyBinding) -> Result<HotKey, String> {
    if binding.key.trim().is_empty() {
        return Err("missing key".to_owned());
    }

    let mut modifiers = Modifiers::empty();
    if binding.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if binding.control {
        modifiers |= Modifiers::CONTROL;
    }
    if binding.alt {
        modifiers |= Modifiers::ALT;
    }
    if binding.super_key {
        modifiers |= Modifiers::SUPER;
    }

    binding
        .canonical()
        .parse::<HotKey>()
        .or_else(|_| HotKey::try_from(binding.key.clone()))
        .map(|hotkey| HotKey::new(Some(modifiers), hotkey.key))
        .map_err(|err| err.to_string())
}
