use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    mem,
};

type FrameId = u32;
// x11 window id
type WindowId = u32;

pub enum LayoutType {
    Horizontal,
    Vertical,
    Floating,
    Tabbed,
}

pub enum ClientStatus {
    Unmapped,
    Mapped,
    WaitForMapped,
}

pub struct Geometry {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

//
// act as tree node
/// A container represents a window or a frame of windows.
/// A container can either be a leaf node (in which case it has a client)
/// or a non-leaf node (in which case it has children).

pub struct Container {
    pub frame_win_id: Option<FrameId>,
    pub children: Vec<Container>,
    pub status: ClientStatus,
    pub main_win_id: Option<WindowId>,
    pub layout_type: LayoutType,
    geometry: Geometry,
    is_repositioned: bool,
    remove_flag: bool,
}

trait Framable {
    fn get_frame_id(&self) -> Option<FrameId>;
}

pub trait Positionable {
    fn get_position(&self) -> (u32, u32);
}

pub trait Dimensionable {
    fn get_dimensions(&self) -> (u32, u32);
}

impl Positionable for Container {
    fn get_position(&self) -> (u32, u32) {
        (self.geometry.x, self.geometry.y)
    }
}

impl Dimensionable for Container {
    fn get_dimensions(&self) -> (u32, u32) {
        (self.geometry.width, self.geometry.height)
    }
}

impl Container {
    pub fn add_child(&mut self, child: Container) -> &mut Container {
        let (x, y) = self.get_position();
        let (width, height) = self.get_dimensions();
        // First iteration: only support Horizontal layout
        self.children.push(child);
        self.reposition();
        return self.children.last_mut().unwrap();
    }

    fn reposition(&mut self) {
        if self.children.is_empty() {
            return;
        }
        let child_width = self.geometry.width / self.children.len() as u32;
        let child_height = self.geometry.height;
        let mut next_x = self.geometry.x;
        self.children.iter_mut().for_each(|c| {
            c.geometry = Geometry {
                x: next_x,
                y: self.geometry.y,
                width: child_width,
                height: child_height,
            };
            c.reposition();
            c.is_repositioned = true;
            next_x += child_width;
        });
    }

    fn get_repositoned_children(&self) -> Vec<&Container> {
        self.children
            .iter()
            .filter(|c| c.is_repositioned)
            .flat_map(|c| {
                if c.children.is_empty() {
                    vec![c]
                } else {
                    c.get_repositoned_children()
                }
            })
            .collect()
    }

    fn get_removed_children(&self) -> Vec<&Container> {
        self.children
            .iter()
            .filter(|c| c.is_repositioned)
            .flat_map(|c| {
                if c.remove_flag {
                    return vec![c];
                }
                return c.get_removed_children();
            })
            .collect()
    }

    fn remove_window(&mut self, window_id: u32) {
        self.children.retain(|c| c.main_win_id != Some(window_id));
        self.reposition();
    }
}

// workspace as a tree
pub struct Workspace {
    // The top window is in front of the vecdeque
    // Top -> ... -> Bottom
    //  0  -> ... ->  n
    display_stack: VecDeque<Container>,
    /// Current focused client
    /// A focused client is always a parent frame that binded to a container
    /// or a application window, both of them are framable
    current_focused: Option<FrameId>,
    // root container
    container: Container,
}

impl Workspace {
    pub fn reposition(&mut self) {
        self.container.reposition();
    }

    fn remove_window(&mut self, window_id: u32) {
        if self.container.main_win_id == Some(window_id) {
            self.container.frame_win_id = None;
            self.container.main_win_id = None;
            return;
        }
        let root_container = &mut self.container as *mut Container;
        let container = self.find_parent_container(root_container, window_id);
        if let Some(container) = container {
            container.remove_window(window_id);
        }
    }

    fn find_parent_container<'a>(
        &'a mut self,
        root: *mut Container,
        id: WindowId,
    ) -> Option<&'a mut Container> {
        let root = unsafe { &mut *root };
        if root.children.iter().any(|c| c.main_win_id == Some(id)) {
            return Some(root);
        }

        let mut found = Err(());

        for (i, child) in root.children.iter_mut().enumerate() {
            if child.main_win_id == Some(id) {
                found = Ok(None);
                break;
            } else if self.find_parent_container(child, id).is_some() {
                found = Ok(Some(i));
                break;
            }
        }
        match found {
            Ok(Some(i)) => Some(&mut root.children[i]),
            Ok(None) => Some(root),
            Err(()) => None,
        }
    }
}

pub struct WmState {
    current_workspace: usize,
    // The number of workspaces
    num_workspaces: usize,
    workspaces: HashMap<usize, Workspace>,
}

impl WmState {
    pub fn new(num_workspaces: usize, width: u32, height: u32) -> Self {
        let mut workspaces = HashMap::new();
        for i in 0..num_workspaces {
            let container = Container {
                frame_win_id: None,
                children: Vec::new(),
                status: ClientStatus::Unmapped,
                main_win_id: None,
                layout_type: LayoutType::Horizontal,
                geometry: Geometry {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                is_repositioned: false,
                remove_flag: false,
            };
            workspaces.insert(
                i,
                Workspace {
                    display_stack: VecDeque::new(),
                    current_focused: None,
                    container,
                },
            );
        }
        Self {
            current_workspace: 0,
            num_workspaces,
            workspaces,
        }
    }

    pub fn new_window(&mut self, client_win_id: u32) -> &mut Container {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        let parent_container = if let Some(current_focused) = workspace.current_focused {
            if workspace.container.frame_win_id == Some(current_focused) {
                &mut workspace.container
            } else {
                workspace
                    .container
                    .children
                    .iter_mut()
                    .find(|c| c.frame_win_id == Some(current_focused))
                    .unwrap()
            }
        } else {
            &mut workspace.container
        };
        let new_container = Container {
            frame_win_id: None,
            children: Vec::new(),
            status: ClientStatus::Mapped,
            main_win_id: Some(client_win_id),
            layout_type: LayoutType::Floating,
            geometry: Geometry {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            is_repositioned: false,
            remove_flag: false,
        };
        let added_container = parent_container.add_child(new_container);
        return added_container;
    }

    pub fn remove_window(&mut self, window_id: WindowId) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        workspace.remove_window(window_id);
    }

    pub fn get_current_workspace(&self) -> &Workspace {
        self.workspaces.get(&self.current_workspace).unwrap()
    }

    pub fn reposition(&mut self) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        workspace.reposition();
    }

    pub fn get_repositioned_windows(&self) -> Vec<&Container> {
        return self
            .get_current_workspace()
            .container
            .get_repositoned_children();
    }

    pub fn get_removed_windows(&self) -> Vec<&Container> {
        return self
            .get_current_workspace()
            .container
            .get_removed_children();
    }

    pub fn change_workspace(&mut self, workspace: usize) {
        self.current_workspace = workspace;
    }

    pub fn move_window_to_left(&mut self, client_win_id: u32) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        let current_focused = workspace.current_focused;
        let parent_container = if let Some(current_focused) = current_focused {
            if workspace.container.frame_win_id == Some(current_focused) {
                &mut workspace.container
            } else {
                workspace
                    .container
                    .children
                    .iter_mut()
                    .find(|c| c.frame_win_id == Some(current_focused))
                    .unwrap()
            }
        } else {
            &mut workspace.container
        };
        let current_container_index = parent_container
            .children
            .iter()
            .position(|c| c.main_win_id == Some(client_win_id))
            .unwrap();
        if current_container_index == 0 {
            return;
        } else {
            parent_container.children[current_container_index].is_repositioned = true;
            parent_container.children[current_container_index - 1].is_repositioned = true;
            parent_container
                .children
                .swap(current_container_index, current_container_index - 1);
        }
    }

    pub fn move_window_to_right(&mut self, client_win_id: u32) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        let current_focused = workspace.current_focused.unwrap();
        let parent_container = if workspace.container.frame_win_id == Some(current_focused) {
            &mut workspace.container
        } else {
            workspace
                .container
                .children
                .iter_mut()
                .find(|c| {
                    c.children
                        .iter()
                        .any(|c| c.frame_win_id == Some(current_focused))
                })
                .unwrap()
        };
        let current_container_index = parent_container
            .children
            .iter()
            .position(|c| c.main_win_id == Some(client_win_id))
            .unwrap();
        if current_container_index == parent_container.children.len() - 1 {
            return;
        } else {
            parent_container.children[current_container_index].is_repositioned = true;
            parent_container.children[current_container_index + 1].is_repositioned = true;
            parent_container
                .children
                .swap(current_container_index, current_container_index + 1);
        }
    }
}
