use std::{
    ops::{Index, IndexMut},
    slice::{Iter, IterMut},
};

use super::common::{FrameId, WindowId};

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

impl Geometry {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}
//
// act as tree node
/// A container represents a window or a frame of windows.
/// A container can either be a leaf node (in which case it has a client)
/// or a non-leaf node (in which case it has children).

#[derive(Debug)]
pub struct Container {
    pub frame_win_id: Option<FrameId>,
    pub main_win_id: Option<WindowId>,
    children: Vec<Container>,
    layout_type: LayoutType,
    geometry: Geometry,
    is_repositioned: bool,
    remove_flag: bool,
    parent: Option<*mut Container>,
}

impl Container {
    pub fn iter(&self) -> Iter<Container> {
        self.children.iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<Container> {
        self.children.iter_mut()
    }
}

impl IndexMut<usize> for Container {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.children[index]
    }
}

impl Index<usize> for Container {
    type Output = Container;

    fn index(&self, index: usize) -> &Self::Output {
        &self.children[index]
    }
}

impl Container {
    pub fn new_without_window(layout_type: LayoutType, geometry: Geometry) -> Self {
        Self {
            frame_win_id: None,
            main_win_id: None,
            children: Vec::new(),
            parent: None,
            layout_type,
            geometry,
            is_repositioned: false,
            remove_flag: false,
        }
    }
    pub fn new(
        frame_win_id: FrameId,
        main_win_id: WindowId,
        layout_type: LayoutType,
        geometry: Geometry,
    ) -> Self {
        Self {
            frame_win_id: Some(frame_win_id),
            main_win_id: Some(main_win_id),
            ..(Self::new_without_window(layout_type, geometry))
        }
    }
    pub fn add_child(&mut self, child: Container) -> &mut Container {
        self.children.push(child);
        self.children.last_mut().unwrap().parent = Some(self as *mut Container);
        self.reposition();
        return self.children.last_mut().unwrap();
    }

    /// Return the next focusing container
    pub(super) fn get_next_focusing_container(&self, window_id: WindowId) -> Option<&Container> {
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

    pub(super) fn clean_removed_children(&mut self) {
        if self.remove_flag {
            return;
        }

        self.children.retain(|c| !c.remove_flag);

        for child in self.children.iter_mut() {
            child.clean_removed_children();
        }
    }

    pub(super) fn mark_removed(&mut self) {
        self.remove_flag = true;
    }

    pub(super) fn unmark_removed(&mut self) {
        self.remove_flag = false;
    }

    /// Get parent container.
    /// It will panic if the container is root container.
    pub(super) fn get_parent(&self) -> *mut Container {
        self.parent.unwrap()
    }

    pub(super) fn try_get_parent(&self) -> Option<*mut Container> {
        self.parent
    }

    pub(super) fn get_repositioned_children(&self) -> Vec<&Container> {
        self.children
            .iter()
            .filter(|c| c.is_repositioned)
            .flat_map(|c| {
                if c.children.is_empty() {
                    vec![c]
                } else {
                    c.get_repositioned_children()
                }
            })
            .collect()
    }

    pub(super) fn get_removed_children(&self) -> Vec<&Container> {
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

    pub(super) fn remove_window(&mut self, window_id: u32) {
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

    pub(super) fn find_child_by_window_id(&self, window_id: u32) -> Option<&Container> {
        if self.main_win_id == Some(window_id) {
            return Some(&self);
        }

        for child in &self.children {
            let found = child.find_child_by_window_id(window_id);
            if let Some(found) = found {
                if found.main_win_id == Some(window_id) {
                    return Some(found);
                }
            }
        }

        return None;
    }

    pub(super) fn find_child_by_frame_id(&self, frame_win_id: FrameId) -> Option<&Container> {
        if self.frame_win_id == Some(frame_win_id) {
            return Some(&self);
        }

        for child in &self.children {
            let found = child.find_child_by_frame_id(frame_win_id);
            if let Some(found) = found {
                if found.frame_win_id == Some(frame_win_id) {
                    return Some(found);
                }
            }
        }

        return None;
    }

    pub(super) fn is_child(&self) -> bool {
        self.parent.is_some()
    }

    pub fn get_dimensions(&self) -> (u32, u32) {
        (self.geometry.width, self.geometry.height)
    }

    pub fn get_position(&self) -> (u32, u32) {
        (self.geometry.x, self.geometry.y)
    }

    pub fn reposition(&mut self) {
        let live_children_count = self.iter().filter(|c| !c.remove_flag).count() as u32;
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
}

// workspace as a tree
