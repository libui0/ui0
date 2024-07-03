const COMPONENTS: &[(&'static str, &'static str)] = ui0_components::load!();

pub struct Component {
    pub name: &'static str,
    pub source: &'static str,
}

impl Component {
    pub fn new(name: &'static str, source: &'static str) -> Self {
        Self { source, name }
    }
}

pub fn get_component(name: &str) -> Option<Component> {
    COMPONENTS
        .iter()
        .map(|c| Component::new(c.0.trim_end_matches(".jsx"), c.1))
        .find(|c| c.name == name)
}
