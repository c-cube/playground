use eframe::egui;
use egui::ViewportBuilder;
use redis::Commands;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

// Structure to hold our application state
struct TaskApp {
    redis_client: redis::Client,
    // Store items with their timestamp ID and all their fields
    items: Vec<(String, HashMap<String, String>)>,
    last_id: String,
    scroll_to_bottom: bool, // New field to track if we need to scroll down
    redis_url: String,      // Store the URL to allow reconnection
}

impl TaskApp {
    fn new(redis_url: &str) -> Result<Self, Box<dyn Error>> {
        let client = redis::Client::open(redis_url)?;
        Ok(TaskApp {
            redis_client: client,
            items: Vec::new(),
            last_id: "0-0".to_string(), // Start from the beginning of the stream
            scroll_to_bottom: false,
            redis_url: redis_url.to_string(),
        })
    }

    fn clear_and_reconnect(&mut self) -> Result<(), Box<dyn Error>> {
        // Create a new client
        self.redis_client = redis::Client::open(self.redis_url.as_str())?;
        // Clear the items list
        self.items.clear();
        // Reset the last_id to start from the beginning
        self.last_id = "0-0".to_string();
        Ok(())
    }

    fn update_tasks(&mut self) -> Result<(), Box<dyn Error>> {
        let mut conn = self.redis_client.get_connection()?;

        // Read from the stream, starting from our last position
        let result: Vec<redis::Value> = conn.xread_options(
            &["tasks"],
            &[&self.last_id],
            &redis::streams::StreamReadOptions::default().count(100),
        )?;

        let mut new_tasks_added = false;

        if !result.is_empty() {
            if let redis::Value::Bulk(streams) = &result[0] {
                if let redis::Value::Bulk(entries) = &streams[1] {
                    for entry in entries {
                        if let redis::Value::Bulk(entry_data) = entry {
                            let mut item_fields = HashMap::new();
                            let mut item_id = String::new();

                            // Update our last_id
                            if let redis::Value::Data(id_bytes) = &entry_data[0] {
                                item_id = String::from_utf8_lossy(id_bytes).to_string();
                                self.last_id = item_id.clone()
                            }

                            // Process the entry data
                            if let redis::Value::Bulk(fields) = &entry_data[1] {
                                for chunk in fields.chunks(2) {
                                    if let (
                                        redis::Value::Data(key_bytes),
                                        redis::Value::Data(value_bytes),
                                    ) = (&chunk[0], &chunk[1])
                                    {
                                        let key = String::from_utf8_lossy(key_bytes).to_string();
                                        let value =
                                            String::from_utf8_lossy(value_bytes).to_string();
                                        item_fields.insert(key, value);
                                    }
                                }
                            }

                            if !item_fields.is_empty() {
                                self.items.push((item_id, item_fields));
                                new_tasks_added = true;
                            }
                        }
                    }
                }
            }
        }

        // Set scroll flag if we got new tasks
        if new_tasks_added {
            self.scroll_to_bottom = true;
        }

        Ok(())
    }
}

impl eframe::App for TaskApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Try to update tasks every frame
        if let Err(e) = self.update_tasks() {
            eprintln!("Error updating tasks: {}", e);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Tasks from Redis Stream");

            // Add the clear/reconnect button in a horizontal layout with the heading
            ui.horizontal(|ui| {
                if ui.button("Clear List & Reconnect").clicked() {
                    if let Err(e) = self.clear_and_reconnect() {
                        eprintln!("Error reconnecting to Redis: {}", e);
                    }
                }
            });
            ui.add_space(8.0); // Add some space between the button and the list

            // Create a scrollable area that takes the remaining space
            egui::ScrollArea::vertical()
                .max_height(ui.available_height()) // Use all available height
                .min_scrolled_height(400.0) // Minimum height to show scrollbar
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    if self.items.is_empty() {
                        ui.label("No items found in the stream.");
                    } else {
                        // Add some padding and show each task in a frame
                        for (idx, (id, fields)) in self.items.iter().enumerate() {
                            ui.add_space(4.0);

                            // Show the stream item ID
                            ui.label(format!("ID: {}", id));
                            ui.separator();

                            // Show all fields in a grid
                            egui::Grid::new(format!("item_grid_{}", idx))
                                .num_columns(2)
                                .spacing([20.0, 4.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    for (key, value) in fields {
                                        ui.label(format!("{}:", key));
                                        ui.label(value);
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(4.0);
                        }

                        // If we need to scroll to bottom, do it now
                        if self.scroll_to_bottom {
                            ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                            self.scroll_to_bottom = false;
                        }
                    }
                });
        });

        // Request a repaint to keep the UI updating
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

fn main() -> Result<(), eframe::Error> {
    let redis_url = "redis://127.0.0.1/";
    let app = TaskApp::new(redis_url).expect("Failed to create TaskApp");

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([400.0, 600.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Task Viewer",
        options,
        Box::new(|_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    )
}
