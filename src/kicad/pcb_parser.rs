use super::{types::*, error::{KicadError, Result}};
use logos::Logos;
use std::collections::HashMap;

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\n\f]+")]
#[logos(skip r"#.*")]
enum Token {
    #[token("(")]
    LParen,
    
    #[token(")")]
    RParen,
    
    #[token("kicad_pcb")]
    KicadPcb,
    
    #[token("version")]
    Version,
    
    #[token("generator")]
    Generator,
    
    #[token("layer")]
    Layer,
    
    #[token("layers")]
    Layers,
    
    #[token("footprint")]
    Footprint,
    
    #[token("segment")]
    Segment,
    
    #[token("via")]
    Via,
    
    #[token("zone")]
    Zone,
    
    #[token("gr_text")]
    GrText,
    
    #[token("gr_line")]
    GrLine,
    
    #[token("gr_circle")]
    GrCircle,
    
    #[token("gr_arc")]
    GrArc,
    
    #[token("gr_rect")]
    GrRect,
    
    #[token("gr_poly")]
    GrPoly,
    
    #[token("at")]
    At,
    
    #[token("size")]
    Size,
    
    #[token("width")]
    Width,
    
    #[token("start")]
    Start,
    
    #[token("end")]
    End,
    
    #[token("center")]
    Center,
    
    #[token("net")]
    Net,
    
    #[token("pad")]
    Pad,
    
    #[token("thru_hole")]
    ThruHole,
    
    #[token("smd")]
    Smd,
    
    #[token("connect")]
    Connect,
    
    #[token("np_thru_hole")]
    NpThruHole,
    
    #[token("drill")]
    Drill,
    
    #[token("rect")]
    Rect,
    
    #[token("circle")]
    Circle,
    
    #[token("oval")]
    Oval,
    
    #[token("roundrect")]
    RoundRect,
    
    #[token("roundrect_rratio")]
    RoundRectRatio,
    
    #[token("polygon")]
    Polygon,
    
    #[token("pts")]
    Pts,
    
    #[token("xy")]
    Xy,
    
    #[token("locked")]
    Locked,
    
    #[token("placed")]
    Placed,
    
    #[token("effects")]
    Effects,
    
    #[token("font")]
    Font,
    
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
    
    #[regex(r"-?\d+(\.\d+)?", |lex| lex.slice().parse::<f64>().ok())]
    Number(f64),
    
    #[regex(r#""([^"\\]|\\.)*""#, |lex| lex.slice()[1..lex.slice().len()-1].to_string())]
    String(String),
    
    
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_\-\.]*", |lex| lex.slice().to_string())]
    Ident(String),
}

pub struct PcbParser {
    tokens: Vec<(Token, String)>,
    position: usize,
}

impl PcbParser {
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
            let context = if self.position > 2 && self.position < self.tokens.len() {
                format!(
                    "Expected {:?}, found {:?} (prev: {:?})",
                    expected,
                    self.current(),
                    self.tokens.get(self.position - 1)
                )
            } else {
                format!(
                    "Expected {:?}, found {:?}",
                    expected,
                    self.current()
                )
            };
            Err(KicadError::UnexpectedToken(context))
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
            _ => {
                let context = if self.position > 0 && self.position < self.tokens.len() {
                    format!("Expected string at position {}, found {:?}", self.position, self.current())
                } else {
                    format!("Expected string, position out of bounds: {}", self.position)
                };
                Err(KicadError::ParseError(context))
            }
        }
    }
    
    fn parse_point(&mut self) -> Result<Point> {
        self.expect(Token::Xy)?;
        let x = self.parse_number()?;
        let y = self.parse_number()?;
        Ok(Point { x, y })
    }
    
    fn parse_at(&mut self) -> Result<(Point, f64)> {
        self.expect(Token::At)?;
        let x = self.parse_number()?;
        let y = self.parse_number()?;
        let rotation = if matches!(self.current(), Some(Token::Number(_))) {
            self.parse_number()?
        } else {
            0.0
        };
        Ok((Point { x, y }, rotation))
    }
    
    fn parse_layer(&mut self) -> Result<Layer> {
        self.expect(Token::LParen)?;
        let id = self.parse_number()? as i32;
        let name = self.parse_string()?;
        let layer_type = self.parse_string()?;
        let mut user_name = None;
        
        // Handle optional user name - could be string or ident
        match self.current() {
            Some(Token::String(_)) | Some(Token::Ident(_)) => {
                user_name = Some(self.parse_string()?);
            }
            _ => {}
        }
        
        self.expect(Token::RParen)?;
        
        Ok(Layer {
            id,
            name,
            layer_type,
            user_name,
        })
    }
    
    fn parse_layers(&mut self) -> Result<HashMap<i32, Layer>> {
        self.expect(Token::LParen)?;
        self.expect(Token::Layers)?;
        
        let mut layers = HashMap::new();
        
        while self.current() == Some(&Token::LParen) {
            let layer = self.parse_layer()?;
            layers.insert(layer.id, layer);
        }
        
        self.expect(Token::RParen)?;
        Ok(layers)
    }
    
    fn parse_pad(&mut self) -> Result<Pad> {
        self.expect(Token::LParen)?;
        self.expect(Token::Pad)?;
        
        let number = self.parse_string()?;
        let pad_type = match self.current() {
            Some(Token::ThruHole) => {
                self.advance();
                "thru_hole".to_string()
            }
            Some(Token::Smd) => {
                self.advance();
                "smd".to_string()
            }
            Some(Token::NpThruHole) => {
                self.advance();
                "np_thru_hole".to_string()
            }
            _ => return Err(KicadError::ParseError("Invalid pad type".to_string())),
        };
        
        let shape = match self.current() {
            Some(Token::Rect) => {
                self.advance();
                "rect".to_string()
            }
            Some(Token::Circle) => {
                self.advance();
                "circle".to_string()
            }
            Some(Token::Oval) => {
                self.advance();
                "oval".to_string()
            }
            Some(Token::RoundRect) => {
                self.advance();
                "roundrect".to_string()
            }
            _ => return Err(KicadError::ParseError("Invalid pad shape".to_string())),
        };
        
        let (position, _) = self.parse_at()?;
        
        self.expect(Token::Size)?;
        let size_x = self.parse_number()?;
        let size_y = self.parse_number()?;
        let size = Point { x: size_x, y: size_y };
        
        let mut drill = None;
        let mut layers = Vec::new();
        let mut net = None;
        let mut roundrect_ratio = None;
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::Drill) => {
                    self.advance();
                    drill = Some(self.parse_number()?);
                }
                Some(Token::LParen) => {
                    self.advance();
                    if self.current() == Some(&Token::Layers) {
                        self.advance();
                        while self.current() != Some(&Token::RParen) {
                            layers.push(self.parse_string()?);
                        }
                        self.advance();
                    } else if self.current() == Some(&Token::Net) {
                        self.advance();
                        let _net_num = self.parse_number()?;
                        net = Some(self.parse_string()?);
                        self.advance();
                    } else if self.current() == Some(&Token::RoundRectRatio) {
                        self.advance();
                        roundrect_ratio = Some(self.parse_number()?);
                        self.advance();
                    } else {
                        self.skip_sexp()?;
                    }
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        
        Ok(Pad {
            number,
            pad_type,
            shape,
            position,
            size,
            drill,
            layers,
            net,
            roundrect_ratio,
        })
    }
    
    fn parse_footprint(&mut self) -> Result<Footprint> {
        self.expect(Token::LParen)?;
        self.expect(Token::Footprint)?;
        
        let name = self.parse_string()?;
        let mut footprint = Footprint {
            name,
            uuid: String::new(),
            position: Point { x: 0.0, y: 0.0 },
            rotation: 0.0,
            layer: String::new(),
            locked: false,
            placed: true,
            properties: HashMap::new(),
            pads: Vec::new(),
            graphics: Vec::new(),
            texts: Vec::new(),
        };
        
        while self.current() != Some(&Token::RParen) {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    match self.current() {
                        Some(Token::At) => {
                            self.position -= 1;
                            let (pos, rot) = self.parse_at()?;
                            footprint.position = pos;
                            footprint.rotation = rot;
                        }
                        Some(Token::Layer) => {
                            self.advance();
                            footprint.layer = self.parse_string()?;
                            self.advance();
                        }
                        Some(Token::Pad) => {
                            self.position -= 1;
                            footprint.pads.push(self.parse_pad()?);
                        }
                        _ => self.skip_sexp()?,
                    }
                }
                Some(Token::Locked) => {
                    self.advance();
                    footprint.locked = true;
                }
                Some(Token::Placed) => {
                    self.advance();
                    footprint.placed = true;
                }
                Some(Token::Ident(s)) if s == "uuid" => {
                    self.advance();
                    footprint.uuid = self.parse_string()?;
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(footprint)
    }
    
    fn parse_track(&mut self) -> Result<Track> {
        self.expect(Token::LParen)?;
        self.expect(Token::Segment)?;
        
        let mut track = Track {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 0.0, y: 0.0 },
            width: 0.0,
            layer: String::new(),
            net: None,
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
                            track.start = Point { x, y };
                            self.advance();
                        }
                        Some(Token::End) => {
                            self.advance();
                            let x = self.parse_number()?;
                            let y = self.parse_number()?;
                            track.end = Point { x, y };
                            self.advance();
                        }
                        Some(Token::Width) => {
                            self.advance();
                            track.width = self.parse_number()?;
                            self.advance();
                        }
                        Some(Token::Layer) => {
                            self.advance();
                            track.layer = self.parse_string()?;
                            self.advance();
                        }
                        Some(Token::Net) => {
                            self.advance();
                            let _net_num = self.parse_number()?;
                            track.net = Some(self.parse_string()?);
                            self.advance();
                        }
                        _ => self.skip_sexp()?,
                    }
                }
                _ => self.advance(),
            }
        }
        
        self.expect(Token::RParen)?;
        Ok(track)
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
    
    fn parse(&mut self) -> Result<PcbFile> {
        self.expect(Token::LParen)?;
        self.expect(Token::KicadPcb)?;
        
        let mut pcb = PcbFile::new();
        
        while self.current() != Some(&Token::RParen) && self.position < self.tokens.len() {
            match self.current() {
                Some(Token::LParen) => {
                    self.advance();
                    match self.current() {
                        Some(Token::Version) => {
                            self.advance();
                            // Version can be a number or string
                            pcb.version = match self.current() {
                                Some(Token::Number(n)) => {
                                    let num = *n;
                                    self.advance();
                                    num.to_string()
                                }
                                _ => self.parse_string()?
                            };
                            self.advance();
                        }
                        Some(Token::Generator) => {
                            self.advance();
                            pcb.generator = self.parse_string()?;
                            self.advance(); // Skip closing paren
                        }
                        Some(Token::Layers) => {
                            self.position -= 1;
                            pcb.layers = self.parse_layers()?;
                        }
                        Some(Token::Footprint) => {
                            self.position -= 1;
                            pcb.footprints.push(self.parse_footprint()?);
                        }
                        Some(Token::Segment) => {
                            self.position -= 1;
                            pcb.tracks.push(self.parse_track()?);
                        }
                        _ => self.skip_sexp()?,
                    }
                }
                _ => self.advance(),
            }
        }
        
        if self.current() == Some(&Token::RParen) {
            self.advance();
        }
        Ok(pcb)
    }
}

impl PcbParser {
    pub fn parse_from_str(content: &str) -> Result<PcbFile> {
        let mut parser = PcbParser::new(content);
        parser.parse()
    }
}

pub fn parse_pcb_for_cam(filename: &str) -> Result<PcbFile> {
    let content = std::fs::read_to_string(filename)?;
    PcbParser::parse_from_str(&content)
}