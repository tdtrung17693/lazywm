use std::collections::{HashMap, VecDeque};

use log::info;

use self::{
    common::WindowId,
    container::{Container, Geometry, LayoutType},
    workspace::Workspace,
};

mod common;
mod container;
mod workspace;

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
            workspaces.insert(i, Workspace::new(width, height));
        }
        Self {
            current_workspace: 0,
            num_workspaces,
            workspaces,
        }
    }

    pub fn new_container(&mut self, client_win_id: u32, frame_win_id: u32) -> &mut Container {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        let new_container = Container::new(
            frame_win_id,
            client_win_id,
            LayoutType::Horizontal,
            Geometry::new(0, 0, 0, 0),
        );
        let added_container = workspace.add_container(new_container);
        return added_container;
    }

    pub fn change_layout(&mut self, layout_type: LayoutType) {
        let workspace = self.get_current_workspace_mut();
        workspace.change_layout(layout_type);
    }

    pub fn remove_container(&mut self, window_id: WindowId) {
        let workspace = self.get_current_workspace_mut();
        workspace.remove_container(window_id);
    }

    pub fn get_current_workspace(&self) -> &Workspace {
        self.workspaces.get(&self.current_workspace).unwrap()
    }

    pub fn get_current_workspace_mut(&mut self) -> &mut Workspace {
        self.workspaces.get_mut(&self.current_workspace).unwrap()
    }

    pub fn reposition(&mut self) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        workspace.reposition();
    }

    pub fn get_repositioned_containers(&self) -> Vec<&Container> {
        return self.get_current_workspace().get_repositioned_children();
    }

    pub fn get_removed_containers(&self) -> Vec<&Container> {
        return self.get_current_workspace().get_removed_children();
    }

    pub fn clean_removed_containers(&mut self) {
        let workspace = self.workspaces.get_mut(&self.current_workspace).unwrap();
        workspace.clean_removed_containers();
    }

    pub fn change_workspace(&mut self, workspace: usize) {
        self.current_workspace = workspace;
    }

    pub fn set_focusing_container(&mut self, window_id: WindowId) {
        let workspace = self.get_current_workspace_mut();
        workspace.set_current_focused_container(window_id);
    }

    pub fn get_focusing_container(&self) -> Option<&Container> {
        let workspace = self.workspaces.get(&self.current_workspace).unwrap();
        let current_focused_container = unsafe { &*workspace.current_focused_container };
        info!(
            "current focused container: {:#?}",
            current_focused_container
        );
        if current_focused_container.frame_win_id.is_some() {
            return Some(current_focused_container);
        }

        return None;
    }

    pub fn move_window_to_left(&mut self, client_win_id: u32) {}

    pub fn move_window_to_right(&mut self, client_win_id: u32) {}
}
