use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use hex;

fn main() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();

    let private_key_bytes = signing_key.to_bytes();
    let public_key_bytes = verifying_key.to_bytes();

    let private_key_hex = hex::encode(private_key_bytes);
    let public_key_hex = hex::encode(public_key_bytes);

    println!("--- Ed25519 Keypair Generated ---");
    println!("PRIVATE KEY (keep this secret, put in .env):");
    println!("LICENSE_PRIVATE_KEY={}", private_key_hex);
    println!();
    println!("PUBLIC KEY (embed this in the client app):");
    println!("LICENSE_PUBLIC_KEY={}", public_key_hex);
    println!("---------------------------------");
}
