fn main() {
    let ctx = pixelmagic_gpu::headless::HeadlessContext::new().expect("egl");
    use glow::HasContext;
    let gl = &ctx.gl;
    unsafe {
        println!("renderer : {}", gl.get_parameter_string(glow::RENDERER));
        println!("version  : {}", gl.get_parameter_string(glow::VERSION));
        println!("glsl     : {}", gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION));
        let maj = gl.get_parameter_i32(glow::MAJOR_VERSION);
        let min = gl.get_parameter_i32(glow::MINOR_VERSION);
        println!("gl       : {maj}.{min}");
        println!(
            "max compute invocations/group : {}",
            gl.get_parameter_i32(glow::MAX_COMPUTE_WORK_GROUP_INVOCATIONS)
        );
        println!(
            "max shared memory bytes       : {}",
            gl.get_parameter_i32(glow::MAX_COMPUTE_SHARED_MEMORY_SIZE)
        );
        println!(
            "max image units               : {}",
            gl.get_parameter_i32(glow::MAX_IMAGE_UNITS)
        );
        println!(
            "max SSBO bindings             : {}",
            gl.get_parameter_i32(glow::MAX_SHADER_STORAGE_BUFFER_BINDINGS)
        );
    }
}
