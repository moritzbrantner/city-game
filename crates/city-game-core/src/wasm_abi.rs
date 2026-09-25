use std::str;

use serde_json::json;

use crate::CitySave;
use crate::browser_transport::{
    execute_json, execute_live_json, new_save_json, prepare_live_render_json, prepare_render_json,
    query_json, query_live_json, render_camera_json, render_frame_json,
    render_frame_with_view_json, save_from_scenario_json,
};

#[unsafe(no_mangle)]
pub extern "C" fn city_game_alloc(len: u32) -> u32 {
    if len == 0 {
        return 0;
    }

    let buffer = vec![0_u8; len as usize].into_boxed_slice();
    Box::into_raw(buffer) as *mut u8 as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_free(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }

    let slice = std::ptr::slice_from_raw_parts_mut(ptr as *mut u8, len as usize);
    // SAFETY: pointers returned by `city_game_alloc` and `write_output` are leaked boxed slices
    // whose exact length is returned to the caller. The browser wrapper frees each allocation once.
    unsafe {
        drop(Box::from_raw(slice));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_new_save(scenario_ptr: u32, scenario_len: u32) -> u64 {
    respond_with_one(scenario_ptr, scenario_len, new_save_json)
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_session_create(scenario_ptr: u32, scenario_len: u32) -> u64 {
    let response = match read_input(scenario_ptr, scenario_len)
        .and_then(|scenario| save_from_scenario_json(&scenario))
    {
        Ok(save) => {
            let handle = Box::into_raw(Box::new(save)) as *mut CitySave as u32;
            serde_json::to_string(&json!({ "ok": true, "handle": handle }))
                .expect("session handle response is serializable")
        }
        Err(error) => error_response(&error),
    };
    write_output(response)
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_session_destroy(handle: u32) {
    if handle == 0 {
        return;
    }

    // SAFETY: the browser wrapper receives this exact pointer from
    // `city_game_session_create`, owns it exclusively, and destroys it at most once.
    unsafe {
        drop(Box::from_raw(handle as *mut CitySave));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_session_execute(
    handle: u32,
    command_ptr: u32,
    command_len: u32,
) -> u64 {
    let response = match read_input(command_ptr, command_len) {
        Ok(command) => match with_live_save_mut(handle, |save| execute_live_json(save, &command)) {
            Ok(response) => response,
            Err(error) => error_response(&error),
        },
        Err(error) => error_response(&error),
    };
    write_output(response)
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_session_query(handle: u32, query_ptr: u32, query_len: u32) -> u64 {
    let response = match read_input(query_ptr, query_len) {
        Ok(query) => match with_live_save(handle, |save| query_live_json(save, &query)) {
            Ok(response) => response,
            Err(error) => error_response(&error),
        },
        Err(error) => error_response(&error),
    };
    write_output(response)
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_session_prepare_render(handle: u32, aspect: f32) -> u64 {
    let response = match with_live_save(handle, |save| prepare_live_render_json(save, aspect)) {
        Ok(response) => response,
        Err(error) => error_response(&error),
    };
    write_output(response)
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_execute(
    save_ptr: u32,
    save_len: u32,
    command_ptr: u32,
    command_len: u32,
) -> u64 {
    respond_with_two(save_ptr, save_len, command_ptr, command_len, execute_json)
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_query(
    save_ptr: u32,
    save_len: u32,
    query_ptr: u32,
    query_len: u32,
) -> u64 {
    respond_with_two(save_ptr, save_len, query_ptr, query_len, query_json)
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_render_frame(save_ptr: u32, save_len: u32, aspect: f32) -> u64 {
    let response = match read_input(save_ptr, save_len) {
        Ok(save) => render_frame_json(&save, aspect),
        Err(error) => error_response(&error),
    };
    write_output(response)
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_prepare_render(save_ptr: u32, save_len: u32, aspect: f32) -> u64 {
    let response = match read_input(save_ptr, save_len) {
        Ok(save) => prepare_render_json(&save, aspect),
        Err(error) => error_response(&error),
    };
    write_output(response)
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_render_camera(
    overview_ptr: u32,
    overview_len: u32,
    view_ptr: u32,
    view_len: u32,
) -> u64 {
    respond_with_two(
        overview_ptr,
        overview_len,
        view_ptr,
        view_len,
        render_camera_json,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn city_game_render_frame_view(
    save_ptr: u32,
    save_len: u32,
    view_ptr: u32,
    view_len: u32,
    aspect: f32,
) -> u64 {
    let response = match (
        read_input(save_ptr, save_len),
        read_input(view_ptr, view_len),
    ) {
        (Ok(save), Ok(view)) => render_frame_with_view_json(&save, &view, aspect),
        (Err(error), _) | (_, Err(error)) => error_response(&error),
    };
    write_output(response)
}

fn with_live_save<T>(handle: u32, use_save: impl FnOnce(&CitySave) -> T) -> Result<T, String> {
    if handle == 0 {
        return Err("city-game session handle must be non-zero".to_owned());
    }

    // SAFETY: session handles are private to the synchronous browser wrapper. The wrapper owns
    // each handle exclusively from create through destroy and rejects use after disposal. The
    // reference is scoped to this call and cannot escape through the closure signature.
    let save = unsafe { &*(handle as *const CitySave) };
    Ok(use_save(save))
}

fn with_live_save_mut<T>(
    handle: u32,
    use_save: impl FnOnce(&mut CitySave) -> T,
) -> Result<T, String> {
    if handle == 0 {
        return Err("city-game session handle must be non-zero".to_owned());
    }

    // SAFETY: the wrapper never aliases mutable session operations. WASM calls are synchronous,
    // one JS CityGameSession owns each handle, and the mutable reference cannot escape this call.
    let save = unsafe { &mut *(handle as *mut CitySave) };
    Ok(use_save(save))
}

fn respond_with_one(ptr: u32, len: u32, handler: fn(&str) -> String) -> u64 {
    let response = match read_input(ptr, len) {
        Ok(input) => handler(&input),
        Err(error) => error_response(&error),
    };
    write_output(response)
}

fn respond_with_two(
    first_ptr: u32,
    first_len: u32,
    second_ptr: u32,
    second_len: u32,
    handler: fn(&str, &str) -> String,
) -> u64 {
    let response = match (
        read_input(first_ptr, first_len),
        read_input(second_ptr, second_len),
    ) {
        (Ok(first), Ok(second)) => handler(&first, &second),
        (Err(error), _) | (_, Err(error)) => error_response(&error),
    };
    write_output(response)
}

fn read_input(ptr: u32, len: u32) -> Result<String, String> {
    if len == 0 {
        return Ok(String::new());
    }
    if ptr == 0 {
        return Err("non-empty WASM input must have a non-zero pointer".to_owned());
    }

    // SAFETY: the browser wrapper obtains input memory through `city_game_alloc`, writes exactly
    // `len` bytes, and keeps the allocation alive for the duration of this synchronous call.
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| format!("WASM input must be UTF-8 JSON: {error}"))
}

fn write_output(output: String) -> u64 {
    let bytes = output.into_bytes().into_boxed_slice();
    let len = u32::try_from(bytes.len()).expect("browser response must fit in wasm32 memory");
    let ptr = Box::into_raw(bytes) as *mut u8 as u32;
    (u64::from(len) << 32) | u64::from(ptr)
}

fn error_response(error: &str) -> String {
    serde_json::to_string(&json!({ "ok": false, "error": error }))
        .expect("WASM error envelope is serializable")
}
