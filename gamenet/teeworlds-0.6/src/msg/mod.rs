use gamenet_common::error::Error;
use packer::Unpacker;
use packer::Warning;
use warn::Warn;

pub mod connless;
pub mod game;
pub mod system;

pub use self::connless::Connless;
pub use self::game::Game;
pub use self::system::System;

pub use gamenet_common::msg::AddrPacked;
pub use gamenet_common::msg::CLIENTS_DATA_NONE;
pub use gamenet_common::msg::ClientsData;
pub use gamenet_common::msg::MessageId;
pub use gamenet_common::msg::SystemOrGame;

struct Protocol;

impl<'a> gamenet_common::msg::Protocol<'a> for Protocol {
    type System = System<'a>;
    type Game = Game<'a>;

    fn decode_system<W>(warn: &mut W, id: MessageId, p: &mut Unpacker<'a>)
        -> Result<Self::System, Error>
        where W: Warn<Warning>
    {
        System::decode_msg(warn, id, p)
    }
    fn decode_game<W>(warn: &mut W, id: MessageId, p: &mut Unpacker<'a>)
        -> Result<Self::Game, Error>
        where W: Warn<Warning>
    {
        Game::decode_msg(warn, id, p)
    }
}

pub fn decode<'a, W>(warn: &mut W, p: &mut Unpacker<'a>)
    -> Result<SystemOrGame<System<'a>, Game<'a>>, Error>
    where W: Warn<Warning>
{
    gamenet_common::msg::decode(warn, Protocol, p)
}

