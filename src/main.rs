#[macro_use]
extern crate serenity;
use serenity::{
  framework::standard::*,
  model::{
    gateway::Ready,
    id::UserId,
  },
  utils::MessageBuilder,
  prelude::*,
  builder::*
};

extern crate lazy_static;
use lazy_static::lazy_static;

extern crate chrono;
use chrono::{Local, DateTime};

extern crate futures;
extern crate futures_state_stream;
extern crate tokio;
use futures::Future;
use futures_state_stream::StateStream;

extern crate tiberius;
use tiberius::SqlConnection;
use tiberius::BoxableIo;

use std::thread;
use std::time::Duration;
use std::sync::atomic::Ordering::*;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::sync::LockResult;
use std::sync::atomic::AtomicBool;

lazy_static! {
    static ref SQLCONN : String = ::std::env::var("SQLCONN").expect("Could not obtain SQLCONN").to_string();
    static ref CHANNEL : String = ::std::env::var("CHANNEL").expect("Could not obtain CHANNEL").to_string();
	static ref GUILDID : String = ::std::env::var("GUILDID").expect("Could not obtain GUILDID").to_string();
    static ref DISCORD_TOKEN : String = ::std::env::var("DISCORD_TOKEN").expect("Could not obtain DISCORD_TOKEN").to_string();
    static ref USERID_STR : String = ::std::env::var("USER_ID").expect("Could not obtain USER_ID").to_string();
    static ref RDY : RwLock<Option<Ready>> = RwLock::new(None);
    static ref THR : AtomicBool = AtomicBool::new(false);
}


fn new_sql() -> SqlConnection<Box<dyn BoxableIo>> {
    let o = SqlConnection::connect(&SQLCONN);
    match o.wait() {
        Ok(k) => k,
        Err(_e) => panic!("Error connecting to SQL Server.")
    }
}

pub struct Handler;

impl EventHandler for Handler {
    fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        update_rdy(ready);
        if THR.load(Acquire) == false {
            thread::spawn(move || {
                let sleeptime = Duration::from_millis(90000);
                loop {
                    let rd : LockResult<RwLockReadGuard<_>> = RDY.read();
                    match rd {
                        Ok(r) => {
                            let rr : RwLockReadGuard<_> = r;
                            match &*rr {
                                Some(rdee) => proc_ready(rdee),
                                None => {}
                            }
                        },
                        Err(_e) => {
                            panic!("Error acquiring read lock!");
                        }
                    }
                    thread::sleep(sleeptime);
                }
            });
            THR.store(true, Release);
        }
    }
}

fn update_rdy(ready: Ready) {
    let rdy : LockResult<RwLockWriteGuard<_>> = RDY.write();
    match rdy {
        Ok(rd) => {
            let mut r : RwLockWriteGuard<_> = rd;
            *r = Some(ready);
        },
        Err(_e) => {
            panic!("Error acquiring write lock!");
        }
    }
}

fn proc_ready(ready: &Ready) {
    let sleeptime = Duration::from_millis(90000);
	let nguildid = &GUILDID.parse::<u64>().expect("Could not parse GUILDID as an unsigned 64-bit integer!");
	match ready.guilds.iter().find(|&x| x.id().as_u64() == nguildid) {
		Some(guild) => {
			match guild.id().channels() {
				Ok(chns) => {
					for key in chns.keys() {
						let n = match key.name() {
							Some(nn) => nn,
							None => { 
								continue  
							}
						};
						if n == *CHANNEL {
							match key.messages(|_| GetMessages::default().limit(1)) {
								Ok(msgs) => {
									if let Some(x) = msgs.last() {
										if let Err(why) = x.delete() {
											println!("Error deleting message: {:?}", why);        
										}
									}
								},
								Err(_) => {}
							}
							let now : DateTime<Local> = Local::now();
							let sqlres = get_users();
							let usercount : i32 = get_user_count();
							let mut toprint : String = format!("**{}** Users online as of {}:\n{}", usercount, now.format("%b/%d/%C %I:%M:%S %P %Z"), sqlres);
							if toprint.len() > 1900 {
								toprint = format!("There are currently too many users online to list, but I *can* tell you there are **{}** users online.", usercount);
							}
							if let Err(why) = key.say(toprint) {
								println!("Error sending message: {:?}", why);
							}
							break;
						}
					}
				},
				Err(arrow) => {
					println!("Error {:?} in proc_ready", arrow);
					thread::sleep(sleeptime);
				}
			}
		},
		None => { 
			println!("Error: Couldn't find guild given as GUILDID in the list of guilds connected to.");
			thread::sleep(sleeptime);
		}
	}
}

fn get_users() -> String {
    let mut output = String::new();
    let mut first : bool = true;
    let rslt = new_sql().simple_query("SELECT AuthName, Name FROM cohdb.dbo.Ents WHERE Active > 0;").for_each(|row| {
        let account : &str = row.get(0);
        let char_name : &str = row.get(1);
        if first == false {
            output.push_str("\n");
        }
        output.push_str(account);
        output.push_str(" is logged in as ");
        output.push_str(char_name);
        if output.len() > 1900 {
            return Err(tiberius::Error::Canceled);
        }
        first = false;
        Ok(())
    }).wait();
    match rslt {
        Ok(_r) => {},
        Err(_e) => println!("Error running query for get_users()")
    }
    output
}

fn get_user(name : String) -> String {
    let nm : &str = &*name;
    let mut output = String::new();
    let rslt = new_sql().query("SELECT AuthName, Name, Active FROM cohdb.dbo.Ents WHERE Active > 0 AND LOWER(AuthName) = LOWER(@P1) OR LOWER(Name) = LOWER(@P1);", &[&nm]).for_each(|row| {
        let account : &str = row.get(0);
        let char_name : &str = row.get(1);
        let active : Option<i32> = row.get(2);
        let logged : bool = match active { 
            Some(a) => a > 0,
            None => false
        };
        if logged == true {
            output.push_str(account);
            output.push_str(" is logged in as ");
            output.push_str(char_name);
        }
        else {
            output.push_str(account);
            output.push_str(" is NOT logged in.");
        }
        
        if output.len() > 1900 {
            output = "Response would be too long! Hacker!".to_string();
        }
        Ok(())
    }).wait();
    match rslt {
        Ok(_r) => {},
        Err(_e) => println!("Error running query for get_user")
    }
    if output.len() == 0 {
        return format!("Couldn't find a character or account named '{}' currently online.", name).to_string();
    }
    output
}

fn get_user_count() -> i32 {
    let mut output : i32 = 0;
    let rslt = new_sql().simple_query("SELECT COUNT(*) FROM cohdb.dbo.Ents WHERE Active > 0;").for_each(|row| {
        output = row.get(0);
        Ok(())
    }).wait();
    match rslt {
        Ok(_r) => {},
        Err(_e) => println!("Error running query for get_user_count()")
    }
    output
}

fn main() {
  let mut client = Client::new(
    &DISCORD_TOKEN,
    Handler,
  )
  .expect("Could not create the client");
  let userid : UserId = UserId(USERID_STR.parse::<u64>().unwrap());
  client.with_framework(
    StandardFramework::new()
      .configure(|c| {
        c.allow_dm(true)
          .allow_whitespace(true)
          .ignore_bots(true)
          .ignore_webhooks(true)
          .on_mention(true)
          .owners(vec![userid].into_iter().collect())
          .prefixes(vec!["!"])
          .no_dm_prefix(true)
          .delimiter(" ")
          .case_insensitivity(true)
      })
      .simple_bucket("help", 30)
      .simple_bucket("queries", 5)
      .help(help_commands::with_embeds)
      .command("ison", |c| c.cmd(ison).desc("Tells you whether a given person is online, by character name or account name.").usage("ison [username or character name]").bucket("queries"))
      // Other
      //.command("as", |c| c.cmd(after_sundown).desc("Rolls After Sundown style").usage("DICE [...]"))
  );

  if let Err(why) = client.start() {
    println!("Client::start error: {:?}", why);
  }
}

command!(ison(_ctx, msg, args) {
    let mb = MessageBuilder::new().mention(&msg.author)
    .push(get_user(args.single::<String>().unwrap()));
    let output = mb.build();
    if let Err(_why) = msg.channel_id.say(output) {
        println!("Error replying to ison");
    }


});
