struct Entry<T> {
    data: Option<T>,
    next: usize,
}

pub struct Slab<T> {
    data: Vec<Entry<T>>,
    next: usize,
    len: usize,
    limit_len: usize,
}
impl<T> Slab<T> {
    pub fn new() -> Slab<T> {
        Slab {
            data: Vec::with_capacity(0),
            next: 0,
            len: 0,
            limit_len: usize::MAX,
        }
    }
    // pub fn with_capacity(size: usize) -> Slab<T> {
    //     Slab {
    //         data: Vec::with_capacity(size),
    //         next: 0,
    //         len: 0,
    //         limit_len: usize::MAX,
    //     }
    // }
    pub fn set_limit_len(&mut self, limit_len: usize) -> &Self {
        self.limit_len = limit_len;
        self
    }
    pub fn push(&mut self, d: T) -> Option<usize> {
        if self.len >= self.limit_len {
            return None;
        }
        self.len += 1;
        let key = self.next;
        if self.data.len() == key {
            self.next = key + 1;
            self.data.push(Entry {
                data: Some(d),
                next: self.next,
            });
            return Some(key);
        } else {
            let mut t = &mut self.data[key];
            t.data = Some(d);
            self.next = t.next;
            return Some(key);
        }
    }
    pub fn get(&self, key: usize) -> Option<&T> {
        if key >= self.data.len() {
            return None;
        }
        let u = &self.data[key];
        u.data.as_ref()
    }

    pub fn remove(&mut self, key: usize) -> Option<T> {
        if key >= self.data.len() {
            return None;
        }
        let d = &mut self.data[key];
        let dt = d.data.take();
        match dt {
            Some(_) => {
                self.len -= 1;
                d.next = self.next;
                self.next = key;
                return dt;
            }
            None => {
                return None;
            }
        }
    }
    // fn len(&self) -> usize {
    //     return self.len;
    // }
}

// #[test]
// fn test() {
//     let mut slab: Slab<i32> = Slab::new();
//     slab.set_limit_len(10);

//     let mut i = 0i32;
//     while i < 10 {
//         assert_eq!(slab.push(i), Some(i as usize));
//         i += 1;
//     }
//     assert_eq!(slab.len(), 10);
//     assert_eq!(slab.push(1), None);
//     assert_eq!(slab.remove(10), None);

//     assert_eq!(slab.get(0), Some(&0));

//     let mut i: usize = 0;
//     while i < 10 {
//         assert_eq!(slab.remove(i), Some(i as i32));
//         i += 1;
//     }
//     assert_eq!(slab.len(), 0);

//     let mut i = 0i32;
//     while i < 10 {
//         assert_eq!(slab.push(i), Some(9 - i as usize));
//         i += 1;
//     }
//     assert_eq!(slab.len(), 10);
// }
