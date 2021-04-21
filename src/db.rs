use rusqlite::{Connection, NO_PARAMS};
use chrono::{Local};
use std::collections::HashMap;

pub struct Db {
    pub conn:Connection,
}

impl Db {
    const CURRENT_VESION:u32 = 3;
    const DEFAULT_ADMIN_USERNAME:&'static str = "admin";
    const DEFAULT_ADMIN_PASSWORD:&'static str = "21232f297a57a5a743894a0e4a801fc3";

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
                self.init_datebase(Some(ret))?;
            },
            Err(_e) => {
                self.init_datebase(None)?;
            }
        };
        Ok(())
    }

    fn init_datebase(&self, database_version: Option<u32>) -> Result<(), Box<dyn std::error::Error>> {
        let mut sqls = HashMap::new();
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let sql1 = format!("insert into version (version,created_at) values ({}, '{}')", Db::CURRENT_VESION, &now);
        let sql2 = format!("insert into admin (username,password,created_at) values ('{}', '{}', '{}')", Db::DEFAULT_ADMIN_USERNAME, Db::DEFAULT_ADMIN_PASSWORD, &now);
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

        sqls.insert(2, vec![
            "drop table if exists cpu_info",
            "drop table if exists memory_info",
            "drop table if exists admin",
            "create table cpu_info (id integer primary key autoincrement,cpu_user real,cpu_system real,cpu_nice real,cpu_idle real,client_id integer not null,created_at datetime)",
            "create table memory_info (id integer primary key autoincrement,memory_free integer,memory_total integer,client_id integer not null,created_at datetime)",
            "alter table client add uptime integer",
            "alter table client add boot_time datetime",
            "create table admin (id integer primary key autoincrement,username varchar(30) not null unique,password CHARACTER(32) not null,last_login_at datetime,created_at datetime)",
            &sql2,
        ]);

        sqls.insert(3, vec![
            "CREATE INDEX c_cid ON cpu_info(client_id)",
            "CREATE INDEX m_cid ON memory_info(client_id)",
        ]);

        sqls.insert(4, vec![
            "alter table client add system_version varchar(50)",
            "alter table client add package_manager_update_count integer",
        ]);

        for (key, value) in sqls {
            match database_version {
                Some(i) => {
                    if key > i {
                        for sql in value {
                            self.conn.execute(sql, NO_PARAMS)?;
                        }
                        self.conn.execute("update version set version=?1", &[key])?;
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
