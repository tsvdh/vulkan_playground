pub mod vertex_shader_module {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "shaders/window_graphics/shader.vert",
        custom_derives: [Default, Copy, Clone],
        define: [("edit_id", "x3axdx7x-xcxx-4axa-aaex-833993bdx87d")]
    }

}

pub mod fragment_shader_module {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "shaders/window_graphics/shader.frag",
        custom_derives: [Default, Copy, Clone],
        define: [("edit_id", "95433xcd-be1e-47a1-8175-7axeb6x28a25")]
    }
}