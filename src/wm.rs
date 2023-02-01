use std::{
    cell::RefCell,
    collections::HashMap,
    process::{Command, Stdio},
    rc::Rc,
};

use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, EnumIter, EnumString};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            ButtonIndex, ButtonMask, ButtonPressEvent, ChangeWindowAttributesAux, Circulate,
            ConfigureRequestEvent, ConfigureWindowAux, ConnectionExt, CreateWindowAux, Cursor,
            EventMask, FocusInEvent, FocusOutEvent, Font, GrabMode, KeyButMask, KeyPressEvent,
            MapRequestEvent, MapState, ModMask, Screen, SetMode, StackMode, UnmapNotifyEvent,
            Window,
        },
        Event,
    },
    rust_connection::RustConnection,
};

use crate::x::{Error, Result};

#[derive(
    AsRefStr, EnumIter, EnumString, Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy,
)]
pub enum Atom {}

pub struct Client {
    frame_win: Window,
    client_win: Window,
}

pub struct WM {
    atoms: HashMap<Atom, u32>,
    conn: RustConnection,
    screen_num: usize,
    clients: RefCell<HashMap<Window, Rc<Client>>>,
    running: RefCell<bool>,
    focusing_client: RefCell<Option<Rc<Client>>>,
    // Stack of original client window
    display_stack: RefCell<Vec<Window>>,
    normal_cursor: Cursor,
}

impl WM {
    pub fn new() -> Result<Self> {
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

        Ok(Self {
            atoms,
            conn,
            clients: RefCell::new(HashMap::new()),
            screen_num,
            running: RefCell::new(false),
            focusing_client: RefCell::new(None),
            normal_cursor,
            display_stack: RefCell::new(Vec::new()),
        })
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
        println!("Framing client win: {client_win}");
        println!("Framing win: {frame_win}");

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

        conn.create_window(
            screen.root_depth,
            frame_win,
            screen.root,
            client_win_geometry.x,
            client_win_geometry.y,
            self.screen().width_in_pixels,
            self.screen().height_in_pixels,
            client_win_geometry.border_width,
            client_win_attrs.class,
            screen.root_visual,
            &attrs,
        )
        .unwrap();

        conn.change_save_set(SetMode::INSERT, client_win).unwrap();
        conn.reparent_window(client_win, frame_win, 0, 0).unwrap();
        conn.map_window(frame_win).unwrap();

        let client = Client {
            frame_win,
            client_win,
        };

        self.grab_buttons(frame_win);
        self.grab_keys(frame_win);
        let client = Rc::new(client);
        let focusing_client = client.clone();
        self.clients.borrow_mut().insert(client_win, client);
        conn.map_window(client_win).unwrap();
        *self.focusing_client.borrow_mut() = Some(focusing_client);
        (*self.display_stack.borrow_mut()).push(client_win);
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
        self.conn
            .grab_button(
                false,
                _window,
                EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                self.screen().root,
                self.normal_cursor,
                ButtonIndex::ANY,
                ModMask::ANY,
            )
            .unwrap()
            .check()
            .unwrap();
    }

    fn grab_keys(&self, _window: Window) {
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

        for k in min_keycode..=max_keycode {
            let idx = ((k - min_keycode) as usize) * keysyms_per_keycode;
            let keysym = keymap.keysyms[idx];
            if xkbcommon::xkb::KEY_q == keysym
                || xkbcommon::xkb::KEY_t == keysym
                || xkbcommon::xkb::KEY_w == keysym
                || xkbcommon::xkb::KEY_h == keysym
            {
                conn.grab_key(
                    false,
                    _window,
                    ModMask::M1,
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

    fn handle_unmap_notify(&self, event: UnmapNotifyEvent) {
        let conn = &self.conn;
        let screen = self.screen();

        let mut clients = self.clients.borrow_mut();
        if let Some(client) = clients.get(&event.window) {
            conn.change_save_set(SetMode::DELETE, event.window).unwrap();

            conn.reparent_window(event.window, screen.root, 0, 0)
                .unwrap();
            conn.destroy_window(client.frame_win).unwrap();
            {
                clients.remove(&event.window);
            }
        }
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
        let k = keymap.keysyms[keysym_index];
        if (state) == KeyButMask::MOD1 {
            // for k in &keymap.keysyms[range] {
            if k == xkbcommon::xkb::KEY_w {
                *self.running.borrow_mut() = false;
            }
            if k == xkbcommon::xkb::KEY_t {
                WM::spawn("dmenu_run");
            }
            if k == xkbcommon::xkb::KEY_h {
                self.focus_left();
            }
            if k == xkbcommon::xkb::KEY_q {
                let top_of_stack = self.display_stack.borrow_mut().pop();
                if let Some(win) = top_of_stack {
                    let clients = self.clients.borrow_mut();
                    let client = clients.get(&win).unwrap();
                    println!("Focus: {:?}", client.client_win);
                    conn.kill_client(client.client_win).unwrap();
                }
                self.focus_top();
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
            if let Some(top_of_stack) = stack.pop() {
                stack.insert(0, top_of_stack);
            }
        }
        self.focus_top();
    }

    fn focus(&self, window: Window) {
        let clients = self.clients.borrow();
        let client = clients.get(&window).unwrap();
        println!("Focus: {:?}", client.client_win);
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
        let conn = &self.conn;
        let setup = conn.setup();
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
