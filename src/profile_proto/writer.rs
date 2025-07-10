use std::{collections::HashMap, io::Write, os::raw::c_void};

use protobuf::{CodedOutputStream, Message};

use crate::{
    profile_proto::profile_proto::{Function, Label, Line, Location, Profile, Sample, ValueType},
    profiler::{AllocSymbolFrames, Symbol},
};

struct StringsTable {
    table: Vec<String>,
    cache: HashMap<String, usize>,
}

impl StringsTable {
    fn new() -> Self {
        let mut st = Self {
            table: vec![],
            cache: HashMap::new(),
        };
        st.add(String::new());
        st
    }
}

impl StringsTable {
    fn add(&mut self, s: String) -> usize {
        match self.cache.get(&s) {
            Some(idx) => *idx,
            None => {
                let idx = self.table.len();
                self.table.push(s.clone());
                self.cache.insert(s, idx);
                idx
            }
        }
    }
}

#[derive(Default)]
struct FuncationsTable {
    table: Vec<Function>,
    index: HashMap<*mut c_void, usize>,
}

enum FuncationAdd {
    Exist(usize),
    Insert(usize),
}

impl FuncationsTable {
    pub fn add(&mut self, strings: &mut StringsTable, symbol: Symbol) -> FuncationAdd {
        match self.index.get(&symbol.addr) {
            Some(index) => FuncationAdd::Exist(*index),
            None => {
                let index = self.table.len() + 1;
                let name = strings.add(symbol.name) as _;
                let filename = strings.add(symbol.file_name) as _;
                let func = Function {
                    id: index as _,
                    name,
                    system_name: name,
                    filename,
                    start_line: symbol.line_no as _,
                    special_fields: Default::default(),
                };
                self.table.push(func);
                self.index.insert(symbol.addr, index);
                FuncationAdd::Insert(index)
            }
        }
    }
}

pub struct ProfileProtoWriter<T: Write> {
    strings_table: StringsTable,
    functions_table: FuncationsTable,
    loc_table: Vec<Location>,
    samples: Vec<Sample>,
    writer: T,
}

impl<T: Write> ProfileProtoWriter<T> {
    pub(crate) fn new(writer: T) -> Self {
        Self {
            strings_table: StringsTable::new(),
            functions_table: Default::default(),
            loc_table: Vec::new(),
            samples: Vec::new(),
            writer,
        }
    }

    pub(crate) fn write_symbol_frame(&mut self, symbol_frame: AllocSymbolFrames) {
        let AllocSymbolFrames { ptr, size, frames } = symbol_frame;
        let mut locs = Vec::<u64>::new();

        for frame in frames {
            let line_no = frame.line_no as i64;
            let address = frame.addr as u64;
            let func_index = match self.functions_table.add(&mut self.strings_table, frame) {
                FuncationAdd::Exist(idx) => {
                    locs.push(idx as _);
                    continue;
                }
                FuncationAdd::Insert(idx) => {
                    locs.push(idx as _);
                    idx as u64
                }
            };
            let line = Line {
                function_id: func_index,
                line: line_no,
                ..Default::default()
            };

            let loc = Location {
                id: func_index,
                line: vec![line],
                address: address,
                ..Default::default()
            };
            self.loc_table.push(loc);
        }
        let lab = Label {
            key: self.strings_table.add("block".into()) as _,
            str: self.strings_table.add(format!("{:p}", ptr)) as _,
            ..Default::default()
        };
        let sample = Sample {
            location_id: locs,
            label: vec![lab],
            value: vec![size as i64],
            ..Default::default()
        };
        self.samples.push(sample);
    }

    pub(crate) fn flush(self) -> std::io::Result<()> {
        let Self {
            mut strings_table,
            functions_table,
            loc_table,
            samples,
            mut writer,
        } = self;
        let samples_value = ValueType {
            type_: strings_table.add("space".into()) as _,
            unit: strings_table.add("bytes".into()) as _,
            ..Default::default()
        };

        let profile = Profile {
            sample_type: vec![samples_value],
            sample: samples,
            string_table: strings_table.table,
            function: functions_table.table,
            location: loc_table,
            ..Default::default()
        };
        let mut stream = CodedOutputStream::new(&mut writer);
        profile.write_to_writer(&mut stream)?;
        Ok(())
    }
}
