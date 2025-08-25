use bigdecimal::BigDecimal;
use dotenv::dotenv;
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, query};
use zbus::{Connection, Result, proxy};  
use futures_util::stream::StreamExt;  
use std::env;


#[derive(Deserialize)]
struct YomamaJoke {
    joke: String,
    category: String,
}


async fn fetch_yomama_joke() -> Result<YomamaJoke> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://www.yomama-jokes.com/api/v1/jokes/random/")
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| zbus::Error::Failure(format!("API request failed: {}", e).to_string()))?;
    
    if response.status().is_success() {
        let joke = response.json::<YomamaJoke>()
            .await
            .map_err(|e| zbus::Error::Failure(format!("API request failed: {}", e).to_string()))?;
        Ok(joke)
    } else {
        Err(zbus::Error::Failure(format!("API request failed with status: {}", response.status()).to_string()))
    }
}
  
#[proxy(  
    interface = "org.asamk.Signal",  
    default_service = "org.asamk.Signal",  
    default_path = "/org/asamk/Signal"  
)]  
trait SignalEvents {  
    #[zbus(signal, name = "MessageReceived")]  
    fn message_received(  
        timestamp: i64,  
        sender: String,  
        group_id: Vec<u8>,  
        message: String,  
        attachments: Vec<String>,  
    );  


    // Method (not signal) - no signal attribute needed  
    #[zbus(name = "sendReadReceipt")]  // Only if you need to override the D-Bus name  
    async fn send_read_receipt(  
        &self,  
        recipient: String,  
        target_sent_timestamps: Vec<i64>,  
    ) -> Result<()>;  

    #[zbus(name = "sendMessage")]  // Only if you need to override the D-Bus name  
    async fn send_message(  
        &self,  
        message: String,  
        attachments: Vec<String>,  
        recipient: String,  
    ) -> Result<i64>;

    #[zbus(name = "getContactName")]  // Only if you need to override the D-Bus name  
    async fn get_contact_name(  
        &self,  
        number: String,  
    ) -> Result<String>;

    #[zbus(name = "sendGroupMessage")]  
    async fn send_group_message(
        &self, message: String, attachments: Vec<String>,
        group_id: Vec<u8>,
    ) -> Result<i64>;

    #[zbus(name = "listNumbers")]
    async fn list_numbers(&self) -> Result<Vec<String>>;
}  

macro_rules! send_response {
    ($proxy:expr, $group_id:expr, $recipient:expr, $message:expr) => {
        if $group_id.len() > 0 {
            $proxy.send_group_message(
                $message,
                vec![],
                $group_id.to_vec()
            ).await?;
        } else {
            $proxy.send_message(
                $message,
                vec![],
                $recipient.to_string()
            ).await?;
        }
    };
}
  
#[tokio::main]  
async fn main() -> Result<()> {  
    // Check for required environment variables
    if env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
        eprintln!("⚠️  DBUS_SESSION_BUS_ADDRESS environment variable not set!");
        eprintln!("This is usually set automatically when logging into a desktop session.");
        eprintln!("If running from SSH or terminal, try:");
        eprintln!("  source ~/.dbus/session-bus/$(cat /etc/machine-id)-0");
    }

    println!("Starting D-Bus client...");
    println!("Attempting to connect to session bus...");

    let connection = Connection::session().await?;  

    println!("Connected to D-Bus...");
      
    let proxy = SignalEventsProxy::builder(&connection)  
        .build()  
        .await?;  

    println!("Proxy created...");
  
    let mut stream = proxy.receive_message_received().await?;  



    dotenv().ok();
    let conn_str = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = PgPoolOptions::new().connect(&conn_str)
        .await.map_err(|e| zbus::Error::Failure(e.to_string()))?;

  
    println!("Listening for Signal messages. Press Ctrl+C to exit...");  
      
    while let Some(signal) = stream.next().await {  
        let args = signal.args()?;  

        // let's leave them on read! how dare they text skybot

        // nah jk lol
        let contact_name = proxy.get_contact_name(args.sender().to_string()).await?;

        // check if they're in a group
        if args.message().starts_with("!help") {
            proxy.send_read_receipt(args.sender().to_string(), 
                vec![*args.timestamp()]).await?;
            send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                format!("Hello {}, SkyBot is currently useless.", contact_name));

        } else if args.message().starts_with("!list") {
            proxy.send_read_receipt(args.sender().to_string(),
                vec![*args.timestamp()]).await?;
            let numbers = proxy.list_numbers().await?;
            // get the contact name for each number and store in a string
            let mut response = String::new();
            for number in numbers {
                response.push_str(format!("{}: {}\n", proxy.get_contact_name(number.to_string()).await?, number).as_str());
            }

            send_response!(&proxy, args.group_id(), args.sender().to_string(), response);

        } else if args.message().starts_with("!bal") {
            // Parse the balance command: !bal [name]
            let parts: Vec<&str> = args.message().splitn(2, ' ').collect();

            if parts.len() < 2 {
                // insert the user into the database
                let bal = query!("
                    INSERT INTO users (name, identifier) VALUES ($1, $2)
                    ON CONFLICT (identifier) DO UPDATE SET balance = users.balance 
                    RETURNING balance;
                ", contact_name, args.sender())
                .fetch_one(&db)
                .await
                .map_err(|e| zbus::Error::Failure(e.to_string()))?;

                proxy.send_read_receipt(args.sender().to_string(), 
                    vec![*args.timestamp()]).await?;
                send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                    format!("Hello {}, your balance is {}", 
                        contact_name, bal.balance.expect("Not a String?").to_string()));
            } else {
                let name = parts[1].to_string();
                match query!("
                    SELECT balance FROM users WHERE name = $1
                ", name)
                .fetch_one(&db)
                .await {
                    Ok(bal) => {
                        proxy.send_read_receipt(args.sender().to_string(), 
                            vec![*args.timestamp()]).await?;
                        send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                            format!("{}'s balance is {}", 
                                name, bal.balance.expect("Not a String?").to_string()));
                    },
                    Err(_) => {
                        proxy.send_read_receipt(args.sender().to_string(), 
                            vec![*args.timestamp()]).await?;
                        send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                            format!("{} is not in the database", name));
                    }
                } 
            } 
        } else if args.message().starts_with("!give") {
            proxy.send_read_receipt(args.sender().to_string(), 
                vec![*args.timestamp()]).await?;
            // Parse the issue command: !give [amount] [name]
            let parts: Vec<&str> = args.message().splitn(3, ' ').collect();
            
            if parts.len() < 3 {
                send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                    "Usage: !issue [amount] [name]".to_string());
            } else {
                // Parse amount
                let amount = match parts[1].parse::<BigDecimal>() {
                    Ok(amt) => amt,
                    Err(_) => {
                        send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                            "Invalid amount format".to_string());
                        continue;
                    }
                };

                if amount <= BigDecimal::from(0) {
                    send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                        "Invalid amount".to_string());
                    continue;
                }
                
                let name = parts[2].to_string();

                
                // Check if user exists and we have enough money and receipient exists
                let result = match query!("
SELECT 
    (SELECT 1 FROM users WHERE name = $1 AND balance >= $2) IS NOT NULL
    AND
    (SELECT 1 FROM users WHERE name = $3) IS NOT NULL
AS is_valid
                    ",
                    contact_name,
                    amount,
                    name
                )
                .fetch_one(&db)
                .await {
                    Ok(res) => res.is_valid,
                    Err(_) => Some(false)
                };

                match result {
                    Some(true) => {
                        // give money to receipient
                        query!("
                            UPDATE users SET balance = balance + $2 WHERE name = $1
                        ", name, amount)
                        .execute(&db)
                        .await
                        .map_err(|e| zbus::Error::Failure(e.to_string()))?;
                        
                        query!("
                            UPDATE users SET balance = balance - $2 WHERE name = $1
                        ", contact_name, amount)
                        .execute(&db)
                        .await
                        .map_err(|e| zbus::Error::Failure(e.to_string()))?;
                        
                        send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                            format!("{}, you gave {} to {}!", contact_name, amount, name));
                    },
                    Some(false) => {
                        send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                            format!("{} does not have enough money or {} doesn't exist", contact_name, name));
                    }
                    None => panic!("Unexpected result from query"),
                }
            }
        } else if args.message().starts_with("!issue") {
            // Parse the issue command: !issue [amount] [name]
            let parts: Vec<&str> = args.message().splitn(3, ' ').collect();
            
            if parts.len() < 3 {
                proxy.send_read_receipt(args.sender().to_string(), 
                    vec![*args.timestamp()]).await?;
                send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                    "Usage: !issue [amount] [name]".to_string());
            } else {
                // Parse amount
                let amount = match parts[1].parse::<BigDecimal>() {
                    Ok(amt) => amt,
                    Err(_) => {
                        proxy.send_read_receipt(args.sender().to_string(), 
                            vec![*args.timestamp()]).await?;
                        send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                            "Invalid amount format".to_string());
                        continue;
                    }
                };
                
                let name = parts[2].to_string();
                
                // Check if user exists and update balance
                let result = query!(
                    "UPDATE users SET balance = balance + $1 WHERE name = $2 RETURNING identifier",
                    amount,
                    name
                )
                .fetch_optional(&db)
                .await
                .map_err(|e| zbus::Error::Failure(e.to_string()))?;
                
                proxy.send_read_receipt(args.sender().to_string(), 
                    vec![*args.timestamp()]).await?;
                
                if result.is_some() {
                    send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                        format!("Added {} to {}'s balance", amount, name));
                } else {
                    send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                        "That user didn't run !bal yet".to_string());
                }
            }
        } else if args.message().starts_with("!tag oh") {
            proxy.send_read_receipt(args.sender().to_string(), 
                vec![*args.timestamp()]).await?;
            send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                "OHOHOHOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHOHOHOHOOHO".to_string());
        } else if args.message().starts_with("!ym") {
            // Fetch a random joke from the Yomama API
            let joke_result = fetch_yomama_joke().await;
            
            proxy.send_read_receipt(args.sender().to_string(), 
                vec![*args.timestamp()]).await?;
            
            match joke_result {
                Ok(joke) => {
                    send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                        format!("{} (Category: {})", joke.joke, joke.category));
                },
                Err(e) => {
                    send_response!(&proxy, args.group_id(), args.sender().to_string(), 
                        format!("Failed to fetch joke: {}", e));
                }
            }
        }



    }  
  
    Ok(())  
}
