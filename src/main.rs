use std::fmt;

struct Point2D (f64, f64);

impl fmt::Display for Point2D {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({},{})", self.0, self.1)
    }
}

fn main() { 
    let square_points: [Point2D; 4] = [ Point2D(0., 0.), Point2D(1.,0.), Point2D(1.,1.), Point2D(0.,1.) ]; 
    for pt in &square_points {
        println!("{}", pt);
    }
}

