use skia_safe::{path, Path};
use glifparser::{Outline, Contour, PointType, Handle};


use super::*;
use super::vector::*;
use super::rect::*;
use super::bezier::*;

// This struct models a simple piecewise function. It maps 0-1 such that 0 is the beginning of the first curve
// in the collection and 1 is the end of the last. It does not currently support arbitrary cuts.
pub struct Piecewise<T: Evaluate> {
    // this should definitely change to private at some point with an iterator or getter to access
    pub curves: Vec<T>,
}

impl<T: Evaluate> Evaluate for Piecewise<T> {
    // return the x, y of our curve at time t
    fn evaluate(&self, t: f64) -> Vector
    {
        // there needs to be better handling than this probably through a fail/success
        if self.curves.len() == 0 {panic!("Can't evaluate an empty piecewise!")}

        // we multiply t by our segments then subtract the floored version of this value from the original to get
        // our offset t for that curve
        let modified_time = (self.curves.len()) as f64 * t;
        let curve_index = modified_time.floor().min((self.curves.len() - 1) as f64) as usize;
        let offset_time = modified_time - curve_index as f64;

        let ref dir = self.curves[curve_index];

        return dir.evaluate(offset_time);  
    }

    // returns the derivative at time t
    fn derivative(&self, t: f64) -> Vector
    {
        // there needs to be better handling than this probably through a fail/success
        if self.curves.len() == 0 {panic!("Can't find derivative for an empty piecewise!")}

        // we multiply t by our segments then subtract the floored version of this value from the original to get
        // our offset t for that curve
        let modified_time = (self.curves.len()) as f64 * t;
        let curve_index = modified_time.floor().min((self.curves.len() - 1) as f64) as usize;
        let offset_time = modified_time - curve_index as f64;

        let ref dir = self.curves[curve_index];

        return dir.derivative(offset_time);  
    }

    fn bounds(&self) -> Rect {
        // again maybe success/failure? These are mainly here to catch bugs right now.
        if self.curves.len() == 0 {panic!("An empty piecewise knows no bounds!")}

        let mut output = Rect {
            left: f64::INFINITY,
            bottom: f64::INFINITY,
            right: -f64::INFINITY,
            top: -f64::INFINITY,
        };

        for curve in &self.curves {
            output = output.encapsulate_rect(curve.bounds());
        }

        return output;
    }

    fn apply_transform<F>(&self, transform: F) -> Self where F: Fn(&Vector) -> Vector
    {
        let mut output = Vec::new();
        for contour in &self.curves {
            output.push(contour.apply_transform(&transform));
        }

        return Piecewise{
            curves: output,
        };
    }
}

// I want to generalize as much of the functionality in these two typed implementations as possible. Some of the stuff
// like the to and from functions are likely to stay, but I'd really like to genericize subdivide and split.
impl Piecewise<Piecewise<Bezier>>
{
    pub fn to_skpath(&self) -> Path
    {
        let path = Path::new();
        return self.append_to_skpath(path);
    }

    pub fn from_skpath(ipath: &Path) -> Self {
        let mut contours: Vec<Piecewise<Bezier>> = Vec::new();
        let iter = path::Iter::new(ipath, false);
    
        let mut cur_contour: Vec<Bezier> = Vec::new();
        let mut last_point: Vector = Vector{x: 0., y: 0.}; // don't think we need this?
        for (v, vp) in iter {
            match v {
                path::Verb::Move => {
                    if !cur_contour.is_empty() {
                        contours.push(Piecewise { curves: cur_contour })
                    }
    
                    cur_contour = Vec::new();  
                    last_point = Vector::from_skia_point(vp.first().unwrap());
                }
    
                path::Verb::Line => {
                    let lp = Vector::from_skia_point(&vp[0]);
                    let np = Vector::from_skia_point(&vp[1]);
                    cur_contour.push(Bezier::from_control_points(lp, lp, np, np));
                    last_point = np;
                }
    
                path::Verb::Quad => {
                    let lp = last_point;
                    let h2 = Vector::from_skia_point(&vp[0]);
                    let np = Vector::from_skia_point(&vp[1]);
                    cur_contour.push(Bezier::from_control_points(lp, lp, h2, np));
                    last_point = np;
                }
    
                path::Verb::Cubic => {
                    let lp = Vector::from_skia_point(&vp[0]);
                    let h1 = Vector::from_skia_point(&vp[1]);
                    let h2 = Vector::from_skia_point(&vp[2]);
                    let np = Vector::from_skia_point(&vp[3]);
                    cur_contour.push(Bezier::from_control_points(lp, h1, h2, np));
                    last_point = np;
                }
    
                path::Verb::Close => {
                    contours.push(Piecewise { curves: cur_contour.clone()});
                    cur_contour = Vec::new();
                }
                
    
                // I might have to implement more verbs, but at the moment we're just converting
                // from glifparser output and these are all the supported primitives there.
                _ => { println!("{:?} {:?}", v, vp); panic!("Unsupported skia verb in skpath!"); }
            }
        }
    
        if !cur_contour.is_empty() {
            contours.push(Piecewise{ curves: cur_contour });
        }
    
        return Piecewise {
            curves: contours
        }
    }    

    pub fn append_to_skpath(&self, mut skpath: Path) -> Path {
        for contour in &self.curves {
            skpath = contour.append_to_skpath(skpath);
        }

        return skpath;
    }

    pub fn from_outline<U>(outline: &Outline<U>) -> Self
    {   
        let mut ret = Piecewise {
            curves: Vec::new(),
        };
    
        for contour in outline
        {
            ret.curves.push(Piecewise::from_contour(contour));
        }
    
        return ret;
    }

    pub fn to_outline(&self) -> Outline<Option<PointData>>
    {
        let mut output_outline: Outline<Option<PointData>> = Outline::new();

        for contour in &self.curves
        {
            output_outline.push(contour.to_contour());
        }

        return output_outline;
    }

    pub fn subdivide(&self, t: f64) -> Self
    {
        let mut output = Vec::new();
        for contour in &self.curves {
            output.push(contour.subdivide(t));
        }

        return Piecewise{
            curves: output,
        };
    }
}

impl Piecewise<Bezier>
{
    pub fn from_contour<U>(contour: &Contour<U>) -> Self
    {   
        let mut ret = Piecewise {
            curves: Vec::new(),
        };

        let mut lastpoint: Option<&glifparser::Point<U>> = None;

        for point in contour
        {
            match lastpoint
            {
                None => {},
                Some(lastpoint) => {
                    ret.curves.push(Bezier::from(&lastpoint, point));
                }
            }

            lastpoint = Some(point);
        }

        let firstpoint = contour.first().unwrap();
        if firstpoint.ptype != PointType::Move {
            ret.curves.push(Bezier::from(&lastpoint.unwrap(), firstpoint));
        }

        return ret
    }

    pub fn to_contour(&self) -> Contour<Option<PointData>>
    {
        let mut output_contour: Contour<Option<PointData>> = Vec::new();
        let mut last_curve: Option<[Vector; 4]> = None;

        for curve in &self.curves
        {                       
            let control_points = curve.to_control_points();

            let mut new_point = control_points[0].to_point(control_points[1].to_handle(), Handle::Colocated);

            // if this isn't the first point we need to backtrack and set our output point's b handle
            match last_curve
            {
                Some(lc) => {
                    // set the last output point's a handle to match the new curve
                    new_point.b = lc[2].to_handle();
                }
                None => {}
            }

            output_contour.push(new_point);

            last_curve = Some(control_points);
        }

        // we've got to connect the last point and the first point
        output_contour.first_mut().unwrap().b = Vector::to_handle(last_curve.unwrap()[2]);
    
        return output_contour;
    }

    
    pub fn append_to_skpath(&self, mut skpath: Path) -> Path
    {
        let mut first = true;
        for bez in &self.curves {
            let controlp = bez.to_control_points();

            if first {
                skpath.move_to(controlp[0].to_skia_point());
                first = false;
            }
            
            // we've got ourselves a line
            if controlp[0] == controlp[2] && controlp[1] == controlp[3] {
                skpath.line_to(controlp[3].to_skia_point());
            }

            skpath.cubic_to(controlp[1].to_skia_point(), controlp[2].to_skia_point(), controlp[3].to_skia_point());
        }

        return skpath;
    }


    pub fn subdivide(&self, t: f64) -> Piecewise<Bezier>
    {
        let mut new_curves = Vec::new();
        for bez in &self.curves {
            let subdivisions = bez.subdivide(t);

            new_curves.push(subdivisions.0);
            new_curves.push(subdivisions.1);
        }

        return Piecewise {
            curves: new_curves
        }
    }
}