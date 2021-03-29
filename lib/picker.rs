use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};

use crate::{
    command::{CommandManager, CommandSource, CommandSourceIter},
    word_database::{WordDatabase, WordIndicesIter},
};

#[derive(Default, Clone, Copy)]
pub struct PickerEntry<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub score: i64,
}

struct CustomEntry {
    pub name: String,
    pub description: String,
}

enum FilteredEntrySource {
    Custom(usize),
    WordDatabase(usize),
    Command(CommandSource),
}

struct FilteredEntry {
    pub source: FilteredEntrySource,
    pub score: i64,
}

#[derive(Default)]
pub struct Picker {
    matcher: SkimMatcherV2,

    custom_entries_len: usize,
    custom_entries_buffer: Vec<CustomEntry>,
    filtered_entries: Vec<FilteredEntry>,

    cursor: usize,
    scroll: usize,
}

impl Picker {
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn height(&self, max_height: usize) -> usize {
        self.filtered_entries.len().min(max_height)
    }

    pub fn len(&self) -> usize {
        self.filtered_entries.len()
    }

    pub fn fuzzy_match(&self, text: &str, pattern: &str) -> Option<i64> {
        let score = self.matcher.fuzzy_match(text, pattern)?;
        let score = score + (text.len() == pattern.len()) as i64;
        Some(score)
    }

    pub fn move_cursor(&mut self, offset: isize) {
        if self.filtered_entries.is_empty() {
            return;
        }

        let end_index = self.filtered_entries.len() - 1;

        if offset > 0 {
            let mut offset = offset as usize;
            if self.cursor == end_index {
                offset -= 1;
                self.cursor = 0;
            }

            if offset < end_index - self.cursor {
                self.cursor += offset;
            } else {
                self.cursor = end_index;
            }
        } else if offset < 0 {
            let mut offset = (-offset) as usize;
            if self.cursor == 0 {
                offset -= 1;
                self.cursor = end_index;
            }

            if offset < self.cursor {
                self.cursor -= offset;
            } else {
                self.cursor = 0;
            }
        }
    }

    pub fn update_scroll(&mut self, max_height: usize) -> usize {
        let height = self.height(max_height);
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + height as usize {
            self.scroll = self.cursor + 1 - height as usize;
        }
        self.scroll = self
            .scroll
            .min(self.filtered_entries.len().saturating_sub(height));

        height
    }

    pub fn clear(&mut self) {
        self.clear_filtered();
        self.custom_entries_len = 0;
    }

    pub fn add_custom_entry(&mut self, name: &str, description: &str) {
        if self.custom_entries_len < self.custom_entries_buffer.len() {
            let entry = &mut self.custom_entries_buffer[self.custom_entries_len];
            entry.name.clear();
            entry.name.push_str(name);
            entry.description.clear();
            entry.description.push_str(description);
        } else {
            let entry = CustomEntry {
                name: name.into(),
                description: description.into(),
            };
            self.custom_entries_buffer.push(entry);
        }

        self.custom_entries_len += 1;
    }

    pub fn add_custom_entry_filtered(&mut self, name: &str, description: &str, pattern: &str) {
        self.add_custom_entry(name, description);
        if self.filter_custom_entry(self.custom_entries_len - 1, pattern) {
            self.filtered_entries
                .sort_unstable_by(|a, b| b.score.cmp(&a.score));
        }
    }

    pub fn clear_filtered(&mut self) {
        self.filtered_entries.clear();
        self.cursor = 0;
        self.scroll = 0;
    }

    pub fn filter(
        &mut self,
        word_indices: WordIndicesIter,
        command_sources: CommandSourceIter,
        pattern: &str,
    ) {
        self.filtered_entries.clear();

        for (i, word) in word_indices {
            if let Some(score) = self.fuzzy_match(word, pattern) {
                self.filtered_entries.push(FilteredEntry {
                    source: FilteredEntrySource::WordDatabase(i),
                    score,
                });
            }
        }

        for (source, command) in command_sources {
            if let Some(score) = self.fuzzy_match(command, pattern) {
                self.filtered_entries.push(FilteredEntry {
                    source: FilteredEntrySource::Command(source),
                    score,
                });
            }
        }

        for i in 0..self.custom_entries_len {
            self.filter_custom_entry(i, pattern);
        }

        self.filtered_entries
            .sort_unstable_by(|a, b| b.score.cmp(&a.score));
        self.cursor = self
            .cursor
            .min(self.filtered_entries.len().saturating_sub(1));
    }

    fn filter_custom_entry(&mut self, index: usize, pattern: &str) -> bool {
        let entry = &self.custom_entries_buffer[index];
        let name_score = self.fuzzy_match(&entry.name, pattern);
        let description_score = self.fuzzy_match(&entry.description, pattern);

        let score = match (name_score, description_score) {
            (None, None) => return false,
            (None, Some(s)) => s,
            (Some(s), None) => s,
            (Some(a), Some(b)) => a.max(b),
        };

        self.filtered_entries.push(FilteredEntry {
            source: FilteredEntrySource::Custom(index),
            score,
        });

        true
    }

    pub fn current_entry<'a>(
        &'a self,
        words: &'a WordDatabase,
        commands: &'a CommandManager,
    ) -> Option<PickerEntry<'a>> {
        let entry = self.filtered_entries.get(self.cursor)?;
        let entry = filtered_to_picker_entry(entry, &self.custom_entries_buffer, words, commands);
        Some(entry)
    }

    pub fn entries<'a>(
        &'a self,
        words: &'a WordDatabase,
        commands: &'a CommandManager,
    ) -> impl 'a + ExactSizeIterator<Item = PickerEntry<'a>> {
        let custom_entries = &self.custom_entries_buffer[..];
        self.filtered_entries
            .iter()
            .map(move |e| filtered_to_picker_entry(e, custom_entries, words, commands))
    }
}

fn filtered_to_picker_entry<'a>(
    entry: &FilteredEntry,
    custom_entries: &'a [CustomEntry],
    words: &'a WordDatabase,
    commands: &'a CommandManager,
) -> PickerEntry<'a> {
    match entry.source {
        FilteredEntrySource::Custom(i) => {
            let e = &custom_entries[i];
            PickerEntry {
                name: &e.name,
                description: &e.description,
                score: entry.score,
            }
        }
        FilteredEntrySource::WordDatabase(i) => PickerEntry {
            name: words.word_at(i),
            description: "",
            score: entry.score,
        },
        FilteredEntrySource::Command(CommandSource::Builtin(i)) => {
            let command = &commands.builtin_commands()[i];
            PickerEntry {
                name: command.name,
                description: command.help.lines().next().unwrap_or(""),
                score: entry.score,
            }
        }
        FilteredEntrySource::Command(CommandSource::Macro(i)) => {
            let command = &commands.macro_commands()[i];
            PickerEntry {
                name: &command.name,
                description: command.help.lines().next().unwrap_or(""),
                score: entry.score,
            }
        }
    }
}
