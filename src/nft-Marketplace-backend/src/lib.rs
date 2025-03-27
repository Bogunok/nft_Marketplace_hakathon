#![allow(clippy::collapsible_else_if)]
#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::string::String;

use candid::{CandidType, Nat, Principal};
use ic_cdk::api;
use ic_cdk::api::caller;
use ic_cdk::api::call;
use std::borrow::Cow;
use ic_cdk::{init, query, update};
use serde::{Deserialize, Serialize};

type TokenId = Nat;
type AccountIdentifier = String; // Represented as a String for simplicity
type Subaccount = [u8; 32];

// Define a type for errors
#[derive(CandidType, Deserialize, Serialize, Debug, PartialEq, Eq)]
pub enum Error {
    Unauthorized,
    InvalidTokenId,
    ZeroAddress,
    OwnerNotFound,
    OperatorNotFound,
    TokenAlreadyExists,
    TransferFailed,
    ApprovalFailed,
    MetadataNotFound,
    NotListedForSale,
    AlreadyListedForSale,
    CannotBuyOwnNFT,
    InsufficientFunds, // Placeholder for payment handling
    Other(String),
}

// Define a result type for canister operations
pub type Result<T, E = Error> = std::result::Result<T, E>;

// Define the structure for NFT metadata (you can customize this)
#[derive(CandidType, serde::Deserialize, Serialize, Clone)]
pub struct Metadata {
    pub name: String,
    pub description: String,
    pub media_url: String,
    // Add other metadata fields as needed
}

// Define the structure for listing information
#[derive(CandidType, Deserialize, Serialize, Clone)]
pub struct Listing {
    pub seller: Principal,
    pub price: Nat, // Price in some unit (e.g., ICP tokens)
}

// Define the state of the canister
#[derive(CandidType, Deserialize, Serialize)]
pub struct State {
    pub name: String,
    pub symbol: String,
    pub owner: Option<Principal>,
    pub total_supply: Nat,
    pub tokens: HashMap<TokenId, Principal>, // Token ID to owner
    pub token_approvals: HashMap<TokenId, Principal>, // Token ID to approved principal
    pub operator_approvals: HashMap<Principal, HashSet<Principal>>, // Owner to set of approved operators
    pub token_metadata: HashMap<TokenId, Metadata>, // Token ID to metadata
    pub next_token_id: Nat,
    pub listings: HashMap<TokenId, Listing>, // Token ID to Listing information
}

impl Default for State {
    fn default() -> Self {
        State {
            name: String::from("MyNFT"),
            symbol: String::from("MNFT"),
            owner: Some(Principal::anonymous()), // Will be set in init
            total_supply: Nat::from(0u32),
            tokens: HashMap::new(),
            token_approvals: HashMap::new(),
            operator_approvals: HashMap::new(),
            token_metadata: HashMap::new(),
            next_token_id: Nat::from(1u32),
            listings: HashMap::new(),
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

#[derive(CandidType, Deserialize)]
struct InitArgs {
    owner: Option<Principal>,
    name: String,
    symbol: String,
}


#[init]
fn init(args: InitArgs) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.owner = Some(args
            .owner
            .unwrap_or_else(api::caller));
        state.name = name();
        state.symbol = symbol();  
    });
}

#[derive(CandidType, Deserialize, Clone)]
struct LogoResult {
    logo_type: Cow<'static, str>,
    data: Cow<'static, str>,
}


#[query(name = "nameDip721")]
fn name() -> String {
    STATE.with(|s| s.borrow().name.clone())
}

#[query(name = "symbolDip721")]
fn symbol() -> String {
    STATE.with(|s| s.borrow().symbol.clone())
}

#[query]
fn totalSupply() -> Nat {
    STATE.with(|s| s.borrow().total_supply.clone())
}

#[query(name = "balanceOfDip721")]
fn balanceOf(owner: Principal) -> Nat {
    STATE.with(|s| {
        s.borrow()
            .tokens
            .values()
            .filter(|&o| o == &owner)
            .count()
            .into()
    })
}

#[query(name = "ownerOfDip721")]
fn ownerOf(token_id: TokenId) -> Result<Principal> {
    STATE.with(|s| {
        s.borrow()
            .tokens
            .get(&token_id)
            .cloned()
            .ok_or(Error::InvalidTokenId)
    })
}

#[update(name = "transferFromDip721")]
fn transferFrom(from: Principal, to: Principal, token_id: TokenId) -> Result<()> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let owner = state.tokens.get(&token_id).ok_or(Error::InvalidTokenId)?;

        if *owner != from && !isApprovedForAllInternal(&state, &from, &caller()) && state.token_approvals.get(&token_id) != Some(&caller()) {
            return Err(Error::Unauthorized);
        }

        if to == Principal::anonymous() {
            return Err(Error::ZeroAddress);
        }

        state.tokens.insert(token_id.clone(), to);
        state.token_approvals.remove(&token_id); // Clear any existing approval
        state.listings.remove(&token_id); // Remove from listings if transferred
        Ok(())
    })
}

#[update(name = "safeTransferFromDip721")]
fn safeTransferFrom(from: Principal, to: Principal, token_id: TokenId) -> Result<()> {
    if to == Principal::anonymous() {
        return Err(Error::ZeroAddress);
    }
    transferFrom(from, to, token_id)
}

#[update(name = "approveDip721")]
fn approve(approved: Principal, token_id: TokenId) -> Result<()> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let owner = state.tokens.get(&token_id).ok_or(Error::InvalidTokenId)?;

        if *owner != caller() && !state.operator_approvals.get(&owner).map_or(false, |operators| operators.contains(&caller())) {
            return Err(Error::Unauthorized);
        }

        state.token_approvals.insert(token_id, approved);
        Ok(())
    })
}

#[query(name = "getApprovedDip721")]
fn getApproved(token_id: TokenId) -> Result<Principal> {
    STATE.with(|s| {
        s.borrow()
            .token_approvals
            .get(&token_id)
            .cloned()
            .ok_or(Error::InvalidTokenId)
    })
}

#[update(name = "setApprovalForAllDip721")]
fn setApprovalForAll(operator: Principal, approved: bool) -> Result<()> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let owner = caller();
        if owner == operator {
            return Ok(()); // Cannot approve yourself
        }
        let operators = state.operator_approvals.entry(owner).or_default();
        if approved {
            operators.insert(operator);
        } else {
            operators.remove(&operator);
        }
        Ok(())
    })
}

#[query(name = "isApprovedForAllDip721")]
fn isApprovedForAll(owner: Principal, operator: Principal) -> bool {
    STATE.with(|s| isApprovedForAllInternal(&s.borrow(), &owner, &operator))
}

fn isApprovedForAllInternal(state: &State, owner: &Principal, operator: &Principal) -> bool {
    state
        .operator_approvals
        .get(owner)
        .map_or(false, |operators| operators.contains(operator))
}

#[update(name = "mintDip721")]
fn mint(to: Principal, metadata: Metadata) -> Result<TokenId> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let token_id = state.next_token_id.clone();
        state.tokens.insert(token_id.clone(), to);
        state.token_metadata.insert(token_id.clone(), metadata);
        state.total_supply = state.total_supply.clone() + Nat::from(1u32);
        state.next_token_id = state.next_token_id.clone() + Nat::from(1u32);
        Ok(token_id)
    })
}

// Placeholder for tokenURI - you'll need to implement the logic to return the actual URI
#[query(name = "tokenURIDip721")]
fn tokenURI(token_id: TokenId) -> Result<String> {
    STATE.with(|s| {
        s.borrow()
            .token_metadata
            .get(&token_id)
            .map(|meta| meta.media_url.clone()) // Or construct the URI based on metadata
            .ok_or(Error::MetadataNotFound)
    })
}

// -------------------- SELLING LOGIC --------------------

// Function to list an NFT for sale
#[update(name = "listItem")]
fn listItem(token_id: TokenId, price: Nat) -> Result<()> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let owner = state.tokens.get(&token_id).ok_or(Error::InvalidTokenId)?;

        if *owner != caller() {
            return Err(Error::Unauthorized);
        }

        if state.listings.contains_key(&token_id) {
            return Err(Error::AlreadyListedForSale);
        }

        state.listings.insert(token_id, Listing { seller: caller(), price });
        Ok(())
    })
}

// Function to delist an NFT from sale
#[update(name = "delistItem")]
fn delistItem(token_id: TokenId) -> Result<()> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let listing = state.listings.get(&token_id).ok_or(Error::NotListedForSale)?;

        if listing.seller != caller() {
            return Err(Error::Unauthorized);
        }

        state.listings.remove(&token_id);
        Ok(())
    })
}

#[query(name = "getListing")]
fn getListing(token_id: TokenId) -> Result<Listing> {
    STATE.with(|s| {
        s.borrow()
            .listings
            .get(&token_id)
            .cloned()
            .ok_or(Error::NotListedForSale)
    })
}

// -------------------- BUYING LOGIC --------------------

#[update(name = "buyItem")]
fn buyItem(token_id: TokenId) -> Result<()> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let listing = state.listings.get(&token_id).ok_or(Error::NotListedForSale)?;
        let buyer = caller();
        let seller = listing.seller; // Get seller before mutable borrow
        let token_id_clone = token_id.clone(); // Clone token_id before mutable borrow

        if seller == buyer {
            return Err(Error::CannotBuyOwnNFT);
        }

        // -------------------- PAYMENT HANDLING (SKIPPED FOR THIS SIMULATION) --------------------
        // In a real-world scenario, you would:
        // 1. Check if the buyer has sufficient funds.
        // 2. Transfer the price amount from the buyer to the seller.
        // -----------------------------------------------------------------------------------------

        // Transfer ownership from the seller to the buyer
        transferFromInternal(&mut state, seller, buyer, token_id_clone)?;

        // Remove the listing after successful purchase
        state.listings.remove(&token_id);

        Ok(())
    })
}

// Internal function to handle transfer, used by buyItem
fn transferFromInternal(
    state: &mut State,
    from: Principal,
    to: Principal,
    token_id: TokenId,
) -> Result<()> {
    let owner = state.tokens.get(&token_id).ok_or(Error::InvalidTokenId)?;

    if *owner != from {
        return Err(Error::Unauthorized);
    }

    if to == Principal::anonymous() {
        return Err(Error::ZeroAddress);
    }

    state.tokens.insert(token_id.clone(), to);
    state.token_approvals.remove(&token_id); // Clear any existing approval
    Ok(())
}

// Example function for setting the NFT name (only callable by the owner)
#[update]
fn setName(new_name: String) -> Result<()> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        if caller() == state.owner.expect("REASON"){
            state.name = new_name;
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    })
}

// Example function for setting the NFT symbol (only callable by the owner)
#[update]
fn setSymbol(new_symbol: String) -> Result<()> {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        if caller() == state.owner.expect("REASON") {
            state.symbol = new_symbol;
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    })
}

// Candid boilerplate
ic_cdk::export_candid!();