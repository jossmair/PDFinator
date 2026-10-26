//! Native open/closed hands: Windows has no predefined grab cursor.
use std::sync::OnceLock;
use windows_sys::Win32::UI::WindowsAndMessaging::HCURSOR;
use super::icon::WinCursor;
use crate::cursor::CursorImage;

pub(super) fn get(closed: bool) -> Option<HCURSOR> {
    static OPEN: OnceLock<WinCursor> = OnceLock::new();
    static CLOSED: OnceLock<WinCursor> = OnceLock::new();
    let cursor = if closed { &CLOSED } else { &OPEN }.get_or_init(|| {
        let rgba = pixels(closed);
        CursorImage::from_rgba(rgba, 32, 32, 15, 16).ok()
            .and_then(|image| WinCursor::new(&image).ok()).unwrap_or(WinCursor::Failed)
    });
    let handle=match cursor { WinCursor::Cursor(cursor) => Some(cursor.as_raw_handle()), _ => None };
    #[cfg(debug_assertions)]
    if let (Some(handle),Some(dir))=(handle,std::env::var_os("PDFINATOR_CURSOR_QA_DIR")) {
        let dir=std::path::Path::new(&dir);
        let name=if closed {"closed"}else{"open"};
        let arrows=unsafe {windows_sys::Win32::UI::WindowsAndMessaging::LoadCursorW(0,windows_sys::Win32::UI::WindowsAndMessaging::IDC_SIZEALL)};
        let _=std::fs::write(dir.join(format!("48-hand-{name}.rgba")),pixels(closed));
        let _=std::fs::write(dir.join(format!("48-hand-{name}.json")),format!("{{\"handle\":{handle},\"arrows\":{arrows},\"width\":32,\"height\":32}}"));
    }
    handle
}

fn pixels(closed: bool) -> Vec<u8> {
    let open: &[[f32;2]] = &[[11.,29.],[10.,24.],[5.,18.],[4.,15.],[5.,13.],[7.,13.],[10.,16.],[10.,6.],[11.,4.],[13.,4.],[14.,6.],[14.,13.],[14.,3.],[15.,2.],[17.,2.],[18.,3.],[18.,13.],[18.,5.],[20.,4.],[22.,5.],[22.,14.],[23.,8.],[25.,7.],[27.,8.],[27.,19.],[26.,23.],[23.,27.],[23.,29.]];
    let fist: &[[f32;2]] = &[[10.,29.],[9.,25.],[5.,19.],[5.,16.],[7.,14.],[10.,16.],[10.,11.],[12.,9.],[15.,9.],[16.,10.],[18.,9.],[21.,9.],[22.,10.],[24.,10.],[26.,12.],[26.,21.],[25.,25.],[23.,27.],[23.,29.]];
    let shape = if closed {fist} else {open};
    let folds: &[([f32;2],[f32;2])] = if closed {&[([12.,12.],[12.,17.]),([16.,12.],[16.,16.]),([20.,12.],[20.,16.]),([10.,18.],[15.,21.])]} else {&[([11.,19.],[14.,22.]),([15.,17.],[22.,17.])]};
    let distance = |p:[f32;2],a:[f32;2],b:[f32;2]| {
        let d=[b[0]-a[0],b[1]-a[1]];
        let t=(((p[0]-a[0])*d[0]+(p[1]-a[1])*d[1])/(d[0]*d[0]+d[1]*d[1]).max(0.001)).clamp(0.,1.);
        (p[0]-a[0]-t*d[0]).hypot(p[1]-a[1]-t*d[1])
    };
    let mut rgba=vec![0;32*32*4];
    for y in 0..32 {for x in 0..32 {
        let mut alpha=0_u32;let mut white=0_u32;
        for sy in 0..4 {for sx in 0..4 {
            let p=[x as f32+(sx as f32+0.5)/4.,y as f32+(sy as f32+0.5)/4.];
            let mut inside=false;let mut border=false;
            for i in 0..shape.len() {
                let a=shape[i];let b=shape[(i+1)%shape.len()];
                if (a[1]>p[1])!=(b[1]>p[1]) && p[0]<(b[0]-a[0])*(p[1]-a[1])/(b[1]-a[1])+a[0] {inside=!inside;}
                border|=distance(p,a,b)<0.65;
            }
            if inside||border {alpha+=255;if !border&&!folds.iter().any(|(a,b)|distance(p,*a,*b)<0.55) {white+=255;}}
        }}
        let i=(y*32+x)*4;
        let c=if alpha>0 {white*255/alpha} else {0} as u8;
        rgba[i..i+4].copy_from_slice(&[c,c,c,(alpha/16) as u8]);
    }}
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_hands_have_distinct_handles_and_are_not_the_move_arrows() {
        let open=get(false).expect("Open hand must be a real Windows cursor");
        let closed=get(true).expect("Closed hand must be a real Windows cursor");
        let arrows=unsafe {windows_sys::Win32::UI::WindowsAndMessaging::LoadCursorW(0,windows_sys::Win32::UI::WindowsAndMessaging::IDC_SIZEALL)};
        assert_ne!(open,0);assert_ne!(closed,0);assert_ne!(open,closed);assert_ne!(open,arrows);
        if let Some(dir)=std::env::var_os("PDFINATOR_CURSOR_QA_DIR") {
            for (name,closed) in [("open",false),("closed",true)] {
                std::fs::write(std::path::Path::new(&dir).join(format!("48-hand-{name}.rgba")),pixels(closed)).unwrap();
            }
        }
    }
}
