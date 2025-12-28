use super::element_rect::ElementRect;

pub struct UiElementIndex;

impl UiElementIndex {
    pub fn new() -> Self {
        Self
    }

    pub fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    pub fn rebuild_index(&mut self, _exclude_hwnd: Option<isize>) -> Result<(), String> {
        Ok(())
    }

    pub fn query_window_at_point(&self, _mx: i32, _my: i32) -> Result<Vec<ElementRect>, String> {
        Ok(Vec::new())
    }

    pub fn query_chain_at_point(
        &mut self,
        _mx: i32,
        _my: i32,
    ) -> Result<Vec<ElementRect>, String> {
        Ok(Vec::new())
    }
}

