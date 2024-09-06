
package main

import "base:runtime"
import "core:fmt"
import "core:math"
import "core:mem"
import "core:strings"
import "core:sync"
import "core:thread"
import "core:time"
import ma "vendor:miniaudio"

OUTPUT_NUM_CHANNELS :: 2
OUTPUT_SAMPLE_RATE :: 48000
PREFERRED_BUFFER_SIZE :: 512

App :: struct {
	// frequency of current note
	hz:     f32,
	hz2:    f32,
	time:   f32,
	device: ma.device,
	mutex:  sync.Mutex,
	sema:   sync.Sema,
}

app: App = {
	time = 0,
	hz   = 440,
	hz2  = 480,
}

main :: proc() {
	result: ma.result

	// set audio device settings
	device_config := ma.device_config_init(ma.device_type.playback)
	device_config.playback.format = ma.format.f32
	device_config.playback.channels = OUTPUT_NUM_CHANNELS
	device_config.sampleRate = OUTPUT_SAMPLE_RATE
	device_config.dataCallback = ma.device_data_proc(audio_callback)
	device_config.periodSizeInFrames = PREFERRED_BUFFER_SIZE

	fmt.println("Configuring MiniAudio Device")
	if (ma.device_init(nil, &device_config, &app.device) != .SUCCESS) {
		fmt.println("Failed to open playback device.")
		return
	}

	// get audio device info just so we can get thre real device buffer size
	{
		info: ma.device_info
		ma.device_get_info(&app.device, ma.device_type.playback, &info)
		//app.buffer_size = int(app.device.playback.internalPeriodSizeInFrames)
		//fmt.printfln("device info: %v", info)
	}


	if (ma.device_start(&app.device) != .SUCCESS) {
		fmt.println("Failed to start playback device.")
		ma.device_uninit(&app.device)
		return
	}
	defer ma.device_uninit(&app.device)
	fmt.println("started")


	notes := [?]f32{62, 66, 69}
	offset_in_notes := 0

	for {
		// fmt.println("main thread sleep")
		time.sleep(600_000_000)
		app.hz = calc_freq_from_midi_note(notes[offset_in_notes])
		fmt.printfln("changed to %f Hz", app.hz)
		app.hz2 = calc_freq_from_midi_note(notes[offset_in_notes]) * 2 * 0
		offset_in_notes = (offset_in_notes + 1) % len(notes)
	}

}

audio_quit :: proc(app: ^App) {
	ma.device_stop(&app.device)
	ma.device_uninit(&app.device)
}

audio_callback :: proc(device: ^ma.device, output, input: rawptr, frame_count: u32) {
	// fmt.printfln("audio cb starting (frame_count=%d)", frame_count)
	buffer_size := int(frame_count * OUTPUT_NUM_CHANNELS)

	// get device buffer
	device_buffer: []f32 = mem.slice_ptr(([^]f32)(output), buffer_size)

	for i in 0 ..< buffer_size / 2 {
		// generate sample from note frequency
		sample1 := math.sin_f32(f32(math.PI) * 2 * app.hz * app.time)
		sample2 := math.sin_f32(f32(math.PI) * 2 * app.hz2 * app.time )

		sample := math.clamp(sample1 + sample2, -1, 1)
		// write sample for both channels
		device_buffer[2 * i] = sample
		device_buffer[2 * i + 1] = sample

		// advance the time
		app.time += 1 / f32(OUTPUT_SAMPLE_RATE)
	}

}

calc_freq_from_midi_note :: proc(note: f32) -> f32 {
	note := note - 9
	hz := 27.5 * math.pow(2, (note / 12))
	fmt.println("New frequency:", hz)
	return hz
}
