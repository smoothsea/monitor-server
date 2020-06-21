use rusqlite::{Connection, NO_PARAMS};
use chrono::{Local};
use std::collections::HashMap;

pub struct Db {
    pub conn:Connection,
}

impl Db {
    const current_version:u32 = 1;

    pub fn get_db() ->  Result<Db, Box<dyn std::error::Error>> {
        Db::new("/data/monitor.db")
    }

    pub fn new(file:&str) -> Result<Db, Box<dyn std::error::Error>> {
        let conn = Connection::open(file)?;
        Ok(Db{
            conn
        })
    }

    pub fn check_init(&self) -> Result<(), Box<dyn std::error::Error>> {
        match self.conn.query_row::<u32, _, _>("select version from version order by id desc", NO_PARAMS, |row| {
           row.get(0)
        }) {
            Ok(ret) => {
                println!("{}", ret);
                self.init_datebase(Some(ret))?;
            },
            Err(e) => {
                self.init_datebase(None)?;
            }
        };
        Ok(())
    }

    fn init_datebase(&self, database_version: Option<u32>) -> Result<(), Box<dyn std::error::Error>> {
        let mut sqls = HashMap::new();
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let sql1 = format!("insert into version (version,created_at) values ({}, '{}')", Db::current_version, &now);
        sqls.insert(1, vec![
            "drop table if exists version",
            "create table version (id integer primary key autoincrement,version integer not null,created_at datetime)",
            &sql1,
            "drop table if exists client",
            "create table client (id integer primary key autoincrement,client_ip integer not null unique,name varchar(30) default null,is_online tinyint default 0,last_online_time datetime,is_enable tiny_int not null default 1,created_at datetime)",
            "drop table if exists task",
            "create table task (id integer primary key autoincrement,client_id integer not null,
                is_valid tiny_int not null default 1,task_type varchar varchar(25) not null,cancled_at datetime,pulled_at datetime,created_at datetime)"
        ]);

        for (key, value) in sqls {
            match database_version {
                Some(i) => {
                    if (Db::current_version > i) {
                        for sql in value {
                            self.conn.execute(sql, NO_PARAMS)?;
                        }
                    }
                },
                None => {
                    for sql in value {
                        self.conn.execute(sql, NO_PARAMS)?;
                    }
                }
            }

        }
        
        Ok(())
    }
}
