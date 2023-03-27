use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    process::{exit, Command, Stdio},
    rc::Rc,
};

use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, EnumIter, EnumString};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            ButtonIndex, ButtonPressEvent, ChangeWindowAttributesAux, Circulate,
            ConfigureRequestEvent, ConfigureWindowAux, ConnectionExt, CreateWindowAux, Cursor,
            EventMask, FocusInEvent, FocusOutEvent, Font, GrabMode, KeyPressEvent, MapRequestEvent,
            MapState, ModMask, Screen, SetMode, StackMode, UnmapNotifyEvent, Window,
        },
        Event,
    },
    rust_connection::RustConnection,
};

use crate::{
    config::Config,
    wm_model::{Dimensionable, Positionable, WmState},
    x::{Error, Result},
};

#[derive(
    AsRefStr, EnumIter, EnumString, Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy,
)]
pub enum Atom {}

pub struct Client {
    frame_win: Window,
    client_win: Window,
}

type Handler = Box<dyn Fn(&WM) -> Result<()>>;

pub struct WM {
    atoms: HashMap<Atom, u32>,
    conn: RustConnection,
    screen_num: usize,
    clients: RefCell<HashMap<Window, Rc<Client>>>,
    window_frame_map: RefCell<HashMap<Window, Window>>,
    running: RefCell<bool>,
    focusing_client: RefCell<Option<Rc<Client>>>,
    // Stack of original client window
    display_stack: RefCell<VecDeque<Window>>,
    normal_cursor: Cursor,
    config: Config,
    commands: HashMap<String, Handler>,
    wm_state: RefCell<WmState>,
    wm_mode: String,
}

impl WM {
    pub fn new(config: Config) -> Result<Self> {
        let (conn, screen_num) = x11rb::connect(None).map_err(Error::from)?;

        let atom_requests = Atom::iter()
            .map(|atom| {
                Ok((
                    atom,
                    conn.intern_atom(false, atom.as_ref().as_bytes())
                        .map_err(Error::from)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let atoms = atom_requests
            .into_iter()
            .map(|(atom, result)| Ok((atom, result.reply().map_err(Error::from)?.atom)))
            .collect::<Result<HashMap<_, _>>>()?;

        let font: Font = conn.generate_id().unwrap();
        conn.open_font(font, b"cursor").unwrap().check().unwrap();
        let normal_cursor: Cursor = conn.generate_id().unwrap();
        conn.create_glyph_cursor(normal_cursor, font, font, 58, 59, 0, 0, 0, 255, 255, 255)
            .unwrap()
            .check()
            .unwrap();
        let commands = Self::build_command_map(config.get_custom_commands());
        let screen = conn.setup().roots.get(screen_num).unwrap();
        let width = screen.width_in_pixels as u32;
        let height = screen.height_in_pixels as u32;
        let wm_state = WmState::new(1, width, height);
        Ok(Self {
            atoms,
            conn,
            clients: RefCell::new(HashMap::new()),
            window_frame_map: RefCell::new(HashMap::new()),
            screen_num,
            running: RefCell::new(false),
            focusing_client: RefCell::new(None),
            normal_cursor,
            display_stack: RefCell::new(VecDeque::new()),
            config,
            commands,
            wm_state: RefCell::new(wm_state),
            wm_mode: "default".into(),
        })
    }

    fn build_command_map<'a>(
        custom_commands: Option<&'a HashMap<String, String>>,
    ) -> HashMap<String, Handler> {
        let mut map: HashMap<String, Handler> = if let Some(custom_commands) = custom_commands {
            custom_commands
                .iter()
                .map(|(k, v)| {
                    let v = v.clone();
                    let handler: Handler = Box::new(move |_wm| {
                        WM::spawn(&v);
                        Ok(())
                    });
                    (k.clone(), handler)
                })
                .collect()
        } else {
            HashMap::new()
        };
        map.insert(
            "quit".into(),
            Box::new(|_| {
                exit(0);
            }),
        );
        map.insert(
            "focus_left".into(),
            Box::new(|wm| {
                wm.focus_left();
                Ok(())
            }),
        );
        map.insert(
            "focus_right".into(),
            Box::new(|wm| {
                wm.focus_right();
                Ok(())
            }),
        );
        map.insert(
            "terminal".into(),
            Box::new(|_| {
                WM::spawn("alacritty");
                Ok(())
            }),
        );
        map.insert(
            "close_window".into(),
            Box::new(|wm| {
                // let top_of_stack = wm.display_stack.borrow_mut().pop_back();
                let wm_state = wm.wm_state.borrow();
                let focusing_container = wm_state.get_focusing_container();
                if let Some(container) = focusing_container {
                    if let Some(window_id) = container.main_win_id {
                        wm.conn.kill_client(window_id).unwrap();
                    } else {
                        wm.conn
                            .kill_client(container.frame_win_id.unwrap())
                            .unwrap();
                    }
                }
                Ok(())
            }),
        );

        map
    }

    pub fn init(&self) {
        let attrs = ChangeWindowAttributesAux::default().event_mask(
            EventMask::SUBSTRUCTURE_REDIRECT
                | EventMask::SUBSTRUCTURE_NOTIFY
                | EventMask::BUTTON_PRESS
                | EventMask::BUTTON_RELEASE
                | EventMask::KEY_PRESS
                | EventMask::KEY_RELEASE,
        );

        self.conn
            .change_window_attributes(self.screen().root, &attrs)
            .unwrap()
            .check()
            .unwrap();

        self.conn.grab_server().unwrap().check().unwrap();
        let tree = self
            .conn
            .query_tree(self.screen().root)
            .unwrap()
            .reply()
            .unwrap();

        for w in tree.children {
            self.frame(w, true);
        }
        self.conn.ungrab_server().unwrap().check().unwrap();
        self.conn.flush().unwrap();
    }

    pub fn run(&self) {
        let conn = &self.conn;
        {
            *self.running.borrow_mut() = true;
        }
        while *self.running.borrow() {
            conn.flush().unwrap();
            let Ok(event) = conn.wait_for_event() else {
                break
            };

            match event {
                Event::MapRequest(xev) => self.handle_map_request(xev),
                Event::ConfigureRequest(xev) => self.handle_configure_request(xev),
                Event::UnmapNotify(xev) => self.handle_unmap_notify(xev),
                Event::KeyPress(xev) => self.handle_key_press(xev),
                Event::ButtonPress(xev) => self.handle_button_press(xev),
                Event::FocusIn(xev) => self.handle_focus_in(xev),
                Event::FocusOut(xev) => self.handle_focus_out(xev),
                _ => {}
            }

            let binding = self.wm_state.borrow_mut();
            let repositioned_windows = binding.get_repositioned_containers();
            repositioned_windows.iter().for_each(|w| {
                let c = *w;
                let (width, height) = c.get_dimensions();
                let (x, y) = c.get_position();
                self.conn
                    .configure_window(
                        w.frame_win_id.unwrap(),
                        &ConfigureWindowAux::new()
                            .width(width)
                            .height(height)
                            .x(x as i32)
                            .y(y as i32),
                    )
                    .unwrap();
                if let Some(main_win_id) = w.main_win_id {
                    self.conn
                        .configure_window(
                            main_win_id,
                            &ConfigureWindowAux::new()
                                .width(width)
                                .height(height)
                                .x(0)
                                .y(0),
                        )
                        .unwrap();
                }
            });
        }
    }

    fn screen(&self) -> &Screen {
        self.conn.setup().roots.get(self.screen_num).unwrap()
    }

    fn handle_map_request(&self, event: MapRequestEvent) {
        let client_win = event.window;
        self.frame(client_win, false);
    }

    fn frame(&self, client_win: Window, scanning: bool) {
        let conn = &self.conn;
        let screen = &conn.setup().roots[self.screen_num];
        let client_win_geometry = conn.get_geometry(client_win).unwrap().reply().unwrap();
        let client_win_attrs = conn
            .get_window_attributes(client_win)
            .unwrap()
            .reply()
            .unwrap();

        if scanning {
            if client_win_attrs.override_redirect
                || client_win_attrs.map_state != MapState::VIEWABLE
            {
                return;
            }
        }

        let frame_win: Window = conn.generate_id().unwrap();

        let config = ConfigureWindowAux::new()
            .width(self.screen().width_in_pixels as u32)
            .height(self.screen().height_in_pixels as u32);
        conn.configure_window(client_win, &config)
            .unwrap()
            .check()
            .unwrap();
        let attrs = CreateWindowAux::new()
            .background_pixel(0x0000ff)
            .border_pixel(0xff0000)
            .event_mask(
                EventMask::SUBSTRUCTURE_REDIRECT
                    | EventMask::SUBSTRUCTURE_NOTIFY
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::KEY_PRESS
                    | EventMask::KEY_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::ENTER_WINDOW,
            );

        let mut wm_state = self.wm_state.borrow_mut();
        let new_container = wm_state.new_container(client_win, frame_win);
        let (width, height) = new_container.get_dimensions();
        let (x, y) = new_container.get_position();
        conn.create_window(
            screen.root_depth,
            frame_win,
            screen.root,
            x as i16,
            y as i16,
            width as u16,
            height as u16,
            client_win_geometry.border_width,
            client_win_attrs.class,
            screen.root_visual,
            &attrs,
        )
        .unwrap();
        conn.change_save_set(SetMode::INSERT, client_win).unwrap();
        conn.reparent_window(client_win, frame_win, 0, 0).unwrap();
        conn.map_window(frame_win).unwrap();

        self.grab_buttons(frame_win);
        self.grab_keys(frame_win, "default");
        conn.map_window(client_win).unwrap();
        self.window_frame_map
            .borrow_mut()
            .insert(client_win, frame_win);
    }

    fn handle_configure_request(&self, event: ConfigureRequestEvent) {
        let conn = &self.conn;
        let configure_attrs = ConfigureWindowAux::from_configure_request(&event);

        if let Some(client) = self.clients.borrow().get(&event.window) {
            conn.configure_window(client.frame_win, &configure_attrs)
                .unwrap();
        }

        conn.configure_window(event.window, &configure_attrs)
            .unwrap();
    }

    fn grab_buttons(&self, _window: Window) {
        // self.conn
        //     .grab_button(
        //         false,
        //         _window,
        //         EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
        //         GrabMode::ASYNC,
        //         GrabMode::ASYNC,
        //         self.screen().root,
        //         self.normal_cursor,
        //         ButtonIndex::ANY,
        //         ModMask::from(self.config.get_mod_mask() as u16),
        //     )
        //     .unwrap()
        //     .check()
        //     .unwrap();
    }

    fn grab_keys(&self, _window: Window, mode: &str) {
        let conn = &self.conn;
        let setup = conn.setup();
        let max_keycode = setup.max_keycode;
        let min_keycode = setup.min_keycode;

        let keymap = conn
            .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)
            .unwrap()
            .reply()
            .unwrap();
        let keysyms_per_keycode = keymap.keysyms_per_keycode as usize;

        //(K - first_code) * keysyms_per_code_return + N
        let config_key_map = self.config.get_key_maps(mode).expect("invalid mode");

        for k in min_keycode..=max_keycode {
            let idx = ((k - min_keycode) as usize) * keysyms_per_keycode;
            let keysym = keymap.keysyms[idx];

            if config_key_map.contains_key(&keysym) {
                let entry = &config_key_map[&keysym];
                for (mod_mask, _) in entry {
                    let mod_mask = mod_mask | self.config.get_mod_mask();
                    conn.grab_key(
                        false,
                        _window,
                        ModMask::from(mod_mask as u16),
                        k,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    )
                    .unwrap()
                    .check()
                    .unwrap();
                }
            }
        }
    }

    fn handle_unmap_notify(&self, event: UnmapNotifyEvent) {
        let conn = &self.conn;
        let screen = self.screen();

        let mut window_frame_map = self.window_frame_map.borrow_mut();
        if let Some(_) = window_frame_map.get(&event.window) {
            conn.change_save_set(SetMode::DELETE, event.window).unwrap();
            conn.reparent_window(event.window, screen.root, 0, 0)
                .unwrap();
            window_frame_map.remove(&event.window);
        } else {
            // remove the frame, no need to continue processing
            return;
        }

        let mut wm_state = self.wm_state.borrow_mut();
        wm_state.remove_container(event.window);
        let removed_containers = wm_state.get_removed_containers();
        // println!(
        //     "removed containers: {:?}",
        //     removed_containers
        //         .iter()
        //         .map(|c| c.frame_win_id)
        //         .collect::<Vec<_>>()
        // );
        for c in removed_containers {
            let Some(frame_win_id ) = c.frame_win_id else { continue;};
            conn.destroy_window(frame_win_id).unwrap();
        }
        wm_state.clean_removed_containers();
    }

    fn handle_key_press(&self, event: KeyPressEvent) {
        let conn = &self.conn;
        let setup = conn.setup();
        let keymap = conn
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)
            .unwrap()
            .reply()
            .unwrap();
        let keysyms_per_keycode = keymap.keysyms_per_keycode as usize;
        let keycode = event.detail as usize;
        let state = event.state;

        //(K - first_code) * keysyms_per_code_return + N
        let keysym_index = (keycode - (setup.min_keycode as usize)) * keysyms_per_keycode;
        let key_sym = keymap.keysyms[keysym_index];
        let key_map = self.config.get_key_maps("default").unwrap();
        let state: u32 = state.into();
        if state != 0 {
            let state = state & (!self.config.get_mod_mask());
            if let Some(mod_map) = key_map.get(&key_sym) {
                if let Some(handler_name) = mod_map.get(&state) {
                    if let Some(handler) = self.commands.get(handler_name) {
                        handler(self).unwrap();
                    }
                }
            }
        } else {
            if key_sym == xkbcommon::xkb::KEY_Escape {
                exit(0);
            }
        }
    }

    fn focus_top(&self) {
        let stack = self.display_stack.borrow();
        let len = stack.len();
        if len > 0 {
            let top_of_stack = stack[len - 1];
            self.focus(top_of_stack);
        }
    }

    fn focus_left(&self) {
        {
            let mut stack = self.display_stack.borrow_mut();
            if let Some(top_of_stack) = stack.pop_back() {
                stack.push_front(top_of_stack);
            }
        }
        self.focus_top();
    }
    fn focus_right(&self) {
        {
            let mut stack = self.display_stack.borrow_mut();
            if let Some(top_of_stack) = stack.pop_front() {
                stack.push_back(top_of_stack);
            }
        }
        self.focus_top();
    }

    fn focus(&self, window: Window) {
        let clients = self.clients.borrow();
        let client = clients.get(&window).unwrap();

        self.conn
            .circulate_window(Circulate::RAISE_LOWEST, client.frame_win)
            .unwrap()
            .check()
            .unwrap();
        let config = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        self.conn
            .configure_window(client.frame_win, &config)
            .unwrap()
            .check()
            .unwrap();
    }

    fn handle_button_press(&self, event: ButtonPressEvent) {
        println!("ButtonClicked on {}", event.event);
    }

    fn spawn<S: Into<String>>(cmd: S) {
        let s = cmd.into();
        let parts: Vec<&str> = s.split_whitespace().collect();
        let result = if parts.len() > 1 {
            Command::new(parts[0])
                .args(&parts[1..])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        } else {
            Command::new(parts[0])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        };

        match result {
            Ok(_) => (),
            Err(e) => panic!("{:?}", e),
        }
    }

    fn handle_focus_in(&self, event: FocusInEvent) {
        println!("FocusIn: {}", event.event);
    }

    fn handle_focus_out(&self, event: FocusOutEvent) {
        println!("FocusOut: {}", event.event);
    }
}
