use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PhongMaterial {
    pub ambient: PhongComponent,
    pub diffuse: PhongComponent,
    pub specular: PhongComponent,
    pub shininess: u32,
}

#[derive(Serialize, Deserialize)]
pub struct PhongComponent {
    pub color: [u8; 3],
    pub coefficient: f32,
}
