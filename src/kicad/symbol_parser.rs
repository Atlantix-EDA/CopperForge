use super::{types::*, error::{KicadError, Result}};
use logos::Logos;

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\n\f]+")]
#[logos(skip r"#.*")]
enum Token {
    #[token("(")]
    LParen,
    
    #[token(")")]
    RParen,
    
    #[token("kicad_symbol_lib")]
    KicadSymbolLib,
    
    #[token("version")]
    Version,
    
    #[token("generator")]
    Generator,
    
    #[token("symbol")]
    Symbol,
    
    #[token("property")]
    Property,
    
    #[token("pin")]
    Pin,
    
    #[token("rectangle")]
    Rectangle,
    
    #[token("circle")]
    Circle,
    
    #[token("arc")]
    Arc,
    
    #[token("polyline")]
    Polyline,
    
    #[token("at")]
    At,
    
    #[token("effects")]
    Effects,
    
    #[token("font")]
    Font,
    
    #[token("size")]
    Size,
    
    #[token("thickness")]
    Thickness,
    
    #[token("bold")]
    Bold,
    
    #[token("italic")]
    Italic,
    
    #[token("justify")]
    Justify,
    
    #[token("hide")]
    Hide,
    
    #[token("start")]
    Start,
    
    #[token("end")]
    End,
    
    #[token("center")]
    Center,
    
    #[token("mid")]
    Mid,
    
    #[token("length")]
    Length,
    
    #[token("stroke")]
    Stroke,
    
    #[token("fill")]
    Fill,
    
    #[token("type")]
    Type,
    
    #[token("color")]
    Color,
    
    #[token("width")]
    Width,
    
    #[token("pts")]
    Pts,
    
    #[token("xy")]
    Xy,
    
    #[token("radius")]
    Radius,
    
    #[token("pin_names_offset")]
    PinNamesOffset,
    
    #[token("in_bom")]
    InBom,
    
    #[token("on_board")]
    OnBoard,
    
    #[token("yes")]
    Yes,
    
    #[token("no")]
    No,
    
    #[regex(r"-?\d+(\.\d+)?", |lex| lex.slice().parse::<f64>().ok())]
    Number(f64),
    
    #[regex(r#""([^"\\]|\\.)*""#, |lex| lex.slice()[1..lex.slice().len()-1].to_string())]
    String(String),
    
    
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_\-\.]*", |lex| lex.slice().to_string())]
    Ident(String),
}

pub struct SymbolParser {
    tokens: Vec<(Token, String)>,
    position: usize,
}

impl SymbolParser {
    fn new(input: &str) -> Self {
        let mut lex = Token::lexer(input);
        let mut tokens = Vec::new();
        
        while let Some(token) = lex.next() {
            if let Ok(token) = token {
                tokens.push((token, lex.slice().to_string()));
            }
        }
        
        Self {
            tokens,
            position: 0,
        }
    }
    
    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.position).map(|(t, _)| t)
    }
    
    fn advance(&mut self) {
        self.position += 1;
    }
    
    fn expect(&mut self, expected: Token) -> Result<()> {
        if self.current() == Some(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(KicadError::UnexpectedToken(format!(
                "Expected {:?}, found {:?}",
                expected,
                self.current()
            )))
        }
    }
    
    fn parse_number(&mut self) -> Result<f64> {
        match self.current() {
            Some(Token::Number(n)) => {
                let num = *n;
                self.advance();
                Ok(num)
            }
            _ => Err(KicadError::ParseError("Expected number".to_string())),
        }
    }
    
    fn parse_string(&mut self) -> Result<String> {
        match self.current() {
            Some(Token::String(s)) => {
                let str = s.clone();
                self.advance();
                Ok(str)
            }
            Some(Token::Ident(s)) => {
                let str = s.clone();
                self.advance();
                Ok(str)
            }
            _ => Err(KicadError::ParseError("Expected string".to_string())),
        }
    }
    
    fn parse_bool(&mut self) -> Result<bool> {
        match self.current() {
            Some(Token::Yes) => {
                self.advance();
                Ok(true)
            }
            Some(Token::No) => {
                self.advance();
                Ok(false)
            }
            _ => Err(KicadError::ParseError("Expected yes/no".to_string())),
        }
    }
    
    fn parse_point(&mut self) -> Result<Point> {
        self.expect(Token::Xy)?;
        let x = self.parse_number()?;
        let y = self.parse_number()?;
        Ok(Point { x, y })
    }
    
    fn parse_at(&mut self) -> Result<Point> {
        self.expect(Token::At)?;
        let x = self.parse_number()?;
        let y = self.parse_number()?;
        let _rotation = if matches!(self.current(), Some(Token::Number(_))) {
            self.parse_number()?
        } else {
            0.0
        };
        Ok(Point { x, y })
    }
    
    fn parse_font(&mut self) -> Result<Font> {
        self.expect(Token::LParen)?;
        self.expect(Token::Font)?;
        
        let mut font = Font {
            size: Point { x: 1.27, y: 1.27 },
            thickness: None,
            bold: false,
            italic: false,
        };
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    if self.current() == Some(&Token::Size) {
                        self.advance();
                        let x = self.parse_number()?;
                        let y = self.parse_number()?;
                        font.size = Point { x, y };
                        self.advance();
                    } else if self.current() == Some(&Token::Thickness) {
                        self.advance();
                        font.thickness = Some(self.parse_number()?);
                        self.advance();
                    } else {
                        self.skip_sexp()?;
                    }
                }
                Some(Token::Bold) => {
                    self.advance();
                    font.bold = true;
                }
                Some(Token::Italic) => {
                    self.advance();
                    font.italic = true;
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(font)
    }
    
    fn parse_effects(&mut self) -> Result<Effects> {
        self.expect(Token::LParen)?;
        self.expect(Token::Effects)?;
        
        let mut effects = Effects {
            font: Font {
                size: Point { x: 1.27, y: 1.27 },
                thickness: None,
                bold: false,
                italic: false,
            },
            justify: None,
            hide: false,
        };
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    if self.current() == Some(&Token::Font) {
                        self.position -= 1;
                        effects.font = self.parse_font()?;
                    } else {
                        self.skip_sexp()?;
                    }
                }
                Some(Token::Justify) => {
                    self.advance();
                    effects.justify = Some(self.parse_string()?);
                }
                Some(Token::Hide) => {
                    self.advance();
                    effects.hide = true;
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(effects)
    }
    
    fn parse_property(&mut self) -> Result<Property> {
        self.expect(Token::LParen)?;
        self.expect(Token::Property)?;
        
        let name = self.parse_string()?;
        let value = self.parse_string()?;
        
        let mut property = Property {
            name,
            value,
            id: 0,
            at: Point { x: 0.0, y: 0.0 },
            effects: None,
        };
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    match self.current() {
                        Some(Token::At) => {
                            self.position -= 1;
                            property.at = self.parse_at()?;
                        }
                        Some(Token::Effects) => {
                            self.position -= 1;
                            property.effects = Some(self.parse_effects()?);
                        }
                        _ => self.skip_sexp()?,
                    }
                }
                Some(Token::Ident(s)) if s == "id" => {
                    self.advance();
                    property.id = self.parse_number()? as i32;
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(property)
    }
    
    fn parse_pin(&mut self) -> Result<Pin> {
        self.expect(Token::LParen)?;
        self.expect(Token::Pin)?;
        
        let pin_type = self.parse_string()?;
        let _shape = self.parse_string()?;
        
        let (at, rotation) = {
            self.expect(Token::LParen)?;
            self.expect(Token::At)?;
            let x = self.parse_number()?;
            let y = self.parse_number()?;
            let rot = if matches!(self.current(), Some(Token::Number(_))) {
                self.parse_number()?
            } else {
                0.0
            };
            self.expect(Token::RParen)?;
            (Point { x, y }, rot)
        };
        
        self.expect(Token::LParen)?;
        self.expect(Token::Length)?;
        let length = self.parse_number()?;
        self.expect(Token::RParen)?;
        
        let mut pin = Pin {
            number: String::new(),
            name: String::new(),
            pin_type,
            at,
            length,
            rotation,
            name_effects: None,
            number_effects: None,
        };
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    if self.current() == Some(&Token::Ident("name".to_string())) {
                        self.advance();
                        pin.name = self.parse_string()?;
                        if self.current() == Some(&Token::LParen) {
                            self.advance();
                            if self.current() == Some(&Token::Effects) {
                                self.position -= 1;
                                pin.name_effects = Some(self.parse_effects()?);
                            } else {
                                self.skip_sexp()?;
                            }
                        }
                        self.advance();
                    } else if self.current() == Some(&Token::Ident("number".to_string())) {
                        self.advance();
                        pin.number = self.parse_string()?;
                        if self.current() == Some(&Token::LParen) {
                            self.advance();
                            if self.current() == Some(&Token::Effects) {
                                self.position -= 1;
                                pin.number_effects = Some(self.parse_effects()?);
                            } else {
                                self.skip_sexp()?;
                            }
                        }
                        self.advance();
                    } else {
                        self.skip_sexp()?;
                    }
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(pin)
    }
    
    fn parse_stroke(&mut self) -> Result<Stroke> {
        self.expect(Token::LParen)?;
        self.expect(Token::Stroke)?;
        
        let mut stroke = Stroke {
            width: 0.0,
            stroke_type: "default".to_string(),
            color: None,
        };
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    match self.current() {
                        Some(Token::Width) => {
                            self.advance();
                            stroke.width = self.parse_number()?;
                            self.advance();
                        }
                        Some(Token::Type) => {
                            self.advance();
                            stroke.stroke_type = self.parse_string()?;
                            self.advance();
                        }
                        _ => self.skip_sexp()?,
                    }
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(stroke)
    }
    
    fn parse_fill(&mut self) -> Result<Fill> {
        self.expect(Token::LParen)?;
        self.expect(Token::Fill)?;
        
        let mut fill = Fill {
            fill_type: "none".to_string(),
            color: None,
        };
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    if self.current() == Some(&Token::Type) {
                        self.advance();
                        fill.fill_type = self.parse_string()?;
                        self.advance();
                    } else {
                        self.skip_sexp()?;
                    }
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(fill)
    }
    
    fn parse_rectangle(&mut self) -> Result<Rectangle> {
        self.expect(Token::LParen)?;
        self.expect(Token::Rectangle)?;
        
        let mut rect = Rectangle {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 0.0, y: 0.0 },
            stroke: Stroke { width: 0.0, stroke_type: "default".to_string(), color: None },
            fill: Fill { fill_type: "none".to_string(), color: None },
        };
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    match self.current() {
                        Some(Token::Start) => {
                            self.advance();
                            let x = self.parse_number()?;
                            let y = self.parse_number()?;
                            rect.start = Point { x, y };
                            self.advance();
                        }
                        Some(Token::End) => {
                            self.advance();
                            let x = self.parse_number()?;
                            let y = self.parse_number()?;
                            rect.end = Point { x, y };
                            self.advance();
                        }
                        Some(Token::Stroke) => {
                            self.position -= 1;
                            rect.stroke = self.parse_stroke()?;
                        }
                        Some(Token::Fill) => {
                            self.position -= 1;
                            rect.fill = self.parse_fill()?;
                        }
                        _ => self.skip_sexp()?,
                    }
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(rect)
    }
    
    fn parse_symbol(&mut self) -> Result<Symbol> {
        self.expect(Token::LParen)?;
        self.expect(Token::Symbol)?;
        
        let name = self.parse_string()?;
        
        let mut symbol = Symbol {
            name,
            pin_names_offset: 0.508,
            in_bom: true,
            on_board: true,
            properties: Vec::new(),
            pins: Vec::new(),
            rectangles: Vec::new(),
            circles: Vec::new(),
            arcs: Vec::new(),
            polylines: Vec::new(),
        };
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    match self.current() {
                        Some(Token::Property) => {
                            self.position -= 1;
                            symbol.properties.push(self.parse_property()?);
                        }
                        Some(Token::Pin) => {
                            self.position -= 1;
                            symbol.pins.push(self.parse_pin()?);
                        }
                        Some(Token::Rectangle) => {
                            self.position -= 1;
                            symbol.rectangles.push(self.parse_rectangle()?);
                        }
                        Some(Token::Symbol) => {
                            self.position -= 1;
                            self.skip_sexp()?;
                        }
                        _ => self.skip_sexp()?,
                    }
                }
                Some(Token::PinNamesOffset) => {
                    self.advance();
                    symbol.pin_names_offset = self.parse_number()?;
                }
                Some(Token::InBom) => {
                    self.advance();
                    symbol.in_bom = self.parse_bool()?;
                }
                Some(Token::OnBoard) => {
                    self.advance();
                    symbol.on_board = self.parse_bool()?;
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(symbol)
    }
    
    fn skip_sexp(&mut self) -> Result<()> {
        let mut depth = 1;
        
        while depth > 0 && self.position < self.tokens.len() {
            match self.current() {
                Some(Token::LParen) => depth += 1,
                Some(Token::RParen) => depth -= 1,
                _ => {}
            }
            self.advance();
        }
        
        Ok(())
    }
    
    fn parse(&mut self) -> Result<Vec<Symbol>> {
        self.expect(Token::LParen)?;
        self.expect(Token::KicadSymbolLib)?;
        
        let mut symbols = Vec::new();
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    match self.current() {
                        Some(Token::Version) => {
                            self.advance();
                            let _version = self.parse_string()?;
                            self.advance();
                        }
                        Some(Token::Generator) => {
                            self.advance();
                            let _generator = self.parse_string()?;
                            self.advance();
                        }
                        Some(Token::Symbol) => {
                            self.position -= 1;
                            symbols.push(self.parse_symbol()?);
                        }
                        _ => self.skip_sexp()?,
                    }
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(symbols)
    }
}

pub fn parse_symbol_lib(filename: &str) -> Result<Vec<Symbol>> {
    let content = std::fs::read_to_string(filename)?;
    let mut parser = SymbolParser::new(&content);
    parser.parse()
}