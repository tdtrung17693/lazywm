use std::collections::{HashMap, VecDeque};

use log::info;

type FrameId = u32;
// x11 window id
type WindowId = u32;

#[derive(Debug)]
pub enum LayoutType {
    Horizontal,
    Vertical,
    Floating,
    Tabbed,
}

impl LayoutType {
    pub fn get_next_geometry(&self, current_geometry: Geometry, unit: Geometry) -> Geometry {
        match &self {
            LayoutType::Horizontal => Geometry {
                x: current_geometry.x + unit.x,
                y: current_geometry.y,
                width: unit.width,
                height: current_geometry.height,
            },
            LayoutType::Vertical => Geometry {
                x: current_geometry.x,
                y: current_geometry.y + unit.y,
                width: current_geometry.width,
                height: unit.height,
            },
            LayoutType::Floating => Geometry {
                x: current_geometry.x,
                y: current_geometry.y,
                width: current_geometry.width,
                height: current_geometry.height,
            },
            LayoutType::Tabbed => Geometry {
                x: current_geometry.x,
                y: current_geometry.y,
                width: current_geometry.width,
                height: current_geometry.height,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug)]
pub struct Container {
    pub frame_win_id: Option<FrameId>,
    children: Vec<Container>,
    pub main_win_id: Option<WindowId>,
    layout_type: LayoutType,
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
        self.children.push(child);
        self.reposition();
        return self.children.last_mut().unwrap();
    }

    fn reposition(&mut self) {
        let live_children_count = self.children.iter().filter(|c| !c.remove_flag).count() as u32;
        if live_children_count == 0 {
            return;
        }
        let child_width = self.geometry.width / live_children_count;
        let child_height = self.geometry.height / live_children_count;
        let mut next_geometry = Geometry {
            x: 0,
            y: 0,
            width: child_width,
            height: self.geometry.height,
        };
        let unit = Geometry {
            x: child_width,
            y: child_height,
            width: child_width,
            height: child_height,
        };
        self.children
            .iter_mut()
            .filter(|c| !c.remove_flag)
            .for_each(|c| {
                c.geometry = next_geometry;
                c.reposition();
                c.is_repositioned = true;
                next_geometry = self.layout_type.get_next_geometry(c.geometry, unit);

                // c.geometry = Geometry {
                //     x: next_x,
                //     y: self.geometry.y,
                //     width: child_width,
                //     height: child_height,
                // };
                // c.reposition();
                // c.is_repositioned = true;
                // next_x += child_width;
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
            .filter(|c| c.remove_flag)
            .flat_map(|c| {
                if c.remove_flag {
                    return vec![c];
                }
                return c.get_removed_children();
            })
            .collect()
    }

    fn remove_window(&mut self, window_id: u32) {
        let index = self
            .children
            .iter()
            .position(|c| c.main_win_id == Some(window_id));
        let Some(index) = index else { return };
        self.children[index].remove_flag = true;

        // clean the container if it has no children
        if self.children.len() - 1 == 0 {
            self.remove_flag = true;
            return;
        }

        // reposition the children
        self.reposition();
    }

    fn find_container_by_window_id(&self, window_id: u32) -> Option<&Container> {
        if self.main_win_id == Some(window_id) {
            return Some(&self);
        }

        for child in &self.children {
            let found = child.find_container_by_window_id(window_id);
            if let Some(found) = found {
                if found.main_win_id == Some(window_id) {
                    return Some(found);
                }
            }
        }

        return None;
    }

    fn find_container_by_frame_id(&self, frame_win_id: FrameId) -> Option<&Container> {
        if self.frame_win_id == Some(frame_win_id) {
            return Some(&self);
        }

        for child in &self.children {
            let found = child.find_container_by_frame_id(frame_win_id);
            if let Some(found) = found {
                if found.frame_win_id == Some(frame_win_id) {
                    return Some(found);
                }
            }
        }

        return None;
    }

    /// Return the next focusing container
    fn get_next_focusing_container(&self, window_id: WindowId) -> Option<&Container> {
        let window_index = self
            .children
            .iter()
            .position(|c| c.main_win_id == Some(window_id))?;

        if self.children.len() == 1 {
            return None;
        } else {
            let index = (window_index + 1) % (self.children.len());
            return Some(&self.children[index]);
        }
    }

    fn clean_removed_children(&mut self) {
        if self.remove_flag {
            return;
        }

        self.children.retain(|c| !c.remove_flag);

        for child in self.children.iter_mut() {
            child.clean_removed_children();
        }
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
    current_focused_frame_id: Option<FrameId>,
    // root container
    container: Container,
}

impl Workspace {
    pub fn reposition(&mut self) {
        self.container.reposition();
    }

    fn remove_container(&mut self, window_id: u32) {
        let root_container = &mut self.container;
        let parent_container =
            Self::find_parent_container(root_container, &mut |c| c.main_win_id == Some(window_id));
        if let Some(parent_container) = parent_container {
            let next_focusing_container = parent_container.get_next_focusing_container(window_id);
            self.current_focused_frame_id =
                if let Some(next_focusing_container) = next_focusing_container {
                    next_focusing_container.frame_win_id
                } else {
                    None
                };

            parent_container.remove_window(window_id);
        }
    }

    // get containers that need to be removed
    // for the X server to clean the corresponding frames
    fn get_removed_containers(&self) -> Vec<&Container> {
        self.container.get_removed_children()
    }

    // actually remove the container from the tree
    fn clean_removed_containers(&mut self) {
        self.container.remove_flag = false;
        self.container.clean_removed_children();
    }

    fn find_parent_container<'a>(
        root: &'a mut Container,
        pred: &impl Fn(&mut Container) -> bool,
    ) -> Option<&'a mut Container> {
        let mut found = Err(());

        for (i, child) in root.children.iter_mut().enumerate() {
            if pred(child) {
                found = Ok(None);
                break;
            } else if Self::find_parent_container(child, pred).is_some() {
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

    fn add_container<'a>(&'a mut self, new_container: Container) -> &'a mut Container {
        let root_container = &mut self.container;
        info!("root container: {:#?}", root_container);
        let parent_container = if let Some(focused_frame_id) = self.current_focused_frame_id {
            Self::find_parent_container(root_container, &|c| {
                c.frame_win_id == Some(focused_frame_id)
            })
        } else {
            Some(root_container)
        };

        self.current_focused_frame_id = Some(new_container.frame_win_id.unwrap());
        let added_container = parent_container.unwrap().add_child(new_container);
        added_container
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
                    current_focused_frame_id: None,
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

    pub fn new_container(&mut self, client_win_id: u32, frame_win_id: u32) -> &mut Container {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        let new_container = Container {
            frame_win_id: Some(frame_win_id),
            children: Vec::new(),
            main_win_id: Some(client_win_id),
            layout_type: LayoutType::Horizontal,
            geometry: Geometry {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            is_repositioned: false,
            remove_flag: false,
        };
        let added_container = workspace.add_container(new_container);
        return added_container;
    }

    pub fn remove_container(&mut self, window_id: WindowId) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        workspace.remove_container(window_id);
    }

    pub fn get_current_workspace(&self) -> &Workspace {
        self.workspaces.get(&self.current_workspace).unwrap()
    }

    pub fn reposition(&mut self) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        workspace.reposition();
    }

    pub fn get_repositioned_containers(&self) -> Vec<&Container> {
        return self
            .get_current_workspace()
            .container
            .get_repositoned_children();
    }

    pub fn get_removed_containers(&self) -> Vec<&Container> {
        return self
            .get_current_workspace()
            .container
            .get_removed_children();
    }

    pub fn clean_removed_containers(&mut self) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        workspace.clean_removed_containers();
    }

    pub fn change_workspace(&mut self, workspace: usize) {
        self.current_workspace = workspace;
    }

    pub fn get_focusing_container(&self) -> Option<&Container> {
        let workspace = self.workspaces.get(&self.current_workspace).unwrap();
        let current_focused = workspace.current_focused_frame_id;
        if let Some(frame_win_id) = current_focused {
            return workspace.container.find_container_by_frame_id(frame_win_id);
        }

        return None;
    }

    pub fn move_window_to_left(&mut self, client_win_id: u32) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        let current_focused = workspace.current_focused_frame_id;
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
        let current_focused = workspace.current_focused_frame_id.unwrap();
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
