use std::collections::VecDeque;

use log::info;

use super::{
    common::{FrameId, WindowId},
    container::{Container, Geometry, LayoutType},
};

pub struct Workspace {
    // The top window is in front of the vecdeque
    // Top -> ... -> Bottom
    //  0  -> ... ->  n
    display_stack: VecDeque<Container>,
    /// Current focused client
    /// A focused client is always a parent frame that binded to a container
    /// or a application window, both of them are framable
    pub(super) current_focused_container: *mut Container,
    // root container
    container: Container,
}

impl Workspace {
    pub fn new(width: u32, height: u32) -> Self {
        let container = Container::new_without_window(
            LayoutType::Horizontal,
            Geometry::new(0, 0, width, height),
        );
        let focus_pointer = &container as *const Container as *mut Container;
        Self {
            display_stack: VecDeque::new(),
            container,
            current_focused_container: focus_pointer,
        }
    }
    pub fn reposition(&mut self) {
        self.container.reposition();
    }

    pub(super) fn remove_container(&mut self, window_id: u32) {
        let root_container = &mut self.container as *mut Container;
        let parent_container =
            Self::find_parent_container(root_container, &mut |c| c.main_win_id == Some(window_id));
        if let Some(parent_container) = parent_container {
            unsafe {
                let parent_container = &mut *parent_container;
                let next_focusing_container =
                    parent_container.get_next_focusing_container(window_id);
                self.current_focused_container =
                    if let Some(next_focusing_container) = next_focusing_container {
                        next_focusing_container as *const Container as *mut Container
                    } else if parent_container.is_child() {
                        parent_container.get_parent() as *const Container as *mut Container
                    } else {
                        root_container
                    };

                parent_container.remove_window(window_id);
            }
        }
    }

    // get containers that need to be removed
    // for the X server to clean the corresponding frames
    fn get_removed_containers(&self) -> Vec<&Container> {
        self.container.get_removed_children()
    }

    // actually remove the container from the tree
    pub(super) fn clean_removed_containers(&mut self) {
        self.container.unmark_removed();
        self.container.clean_removed_children();
    }

    fn find_parent_container<'a>(
        root: *mut Container,
        pred: &impl Fn(&mut Container) -> bool,
    ) -> Option<*mut Container> {
        unsafe {
            let root = &mut *root;
            let mut found = Err(());

            for (i, child) in root.iter_mut().enumerate() {
                if pred(child) {
                    found = Ok(None);
                    break;
                } else if Self::find_parent_container(child, pred).is_some() {
                    found = Ok(Some(i));
                    break;
                }
            }
            match found {
                Ok(Some(i)) => Some(&mut root[i]),
                Ok(None) => Some(root),
                Err(()) => None,
            }
        }
    }

    pub(super) fn add_container<'a>(&'a mut self, new_container: Container) -> &'a mut Container {
        let root_container = &mut self.container;
        let parent_container = unsafe {
            let focusing_container = &mut *self.current_focused_container;
            if focusing_container.is_child() {
                focusing_container.get_parent()
            } else {
                root_container
            }
        };
        info!("root container: {:#?}", parent_container);

        let added_container = unsafe { &mut *parent_container }.add_child(new_container);
        self.current_focused_container = added_container as *mut Container;
        added_container
    }

    pub(super) fn set_current_focused_container(&mut self, window_id: WindowId) {
        let root_container = &mut self.container as *mut Container;
        let Some(parent_container) =
            Self::find_parent_container(root_container, &mut |c| c.main_win_id == Some(window_id)) else {return;};
        let parent_container = unsafe { &mut *parent_container };
        let Some(container) = parent_container
            .iter()
            .find(|&c| c.main_win_id == Some(window_id))
            .map(|c| c as *const Container as *mut Container) else {
                return
            };
        self.current_focused_container = container;
    }

    pub fn get_repositioned_children(&self) -> Vec<&Container> {
        self.container.get_repositioned_children()
    }

    pub(crate) fn get_removed_children(&self) -> Vec<&Container> {
        self.container.get_removed_children()
    }

    pub(crate) fn change_layout(&self, layout_type: LayoutType) {
        todo!()
    }
}
