use crate::device::software::tests::SGListBuilder;

#[test]
fn test_helper_cut_from_sgl() {
    // Here we don't care the va boundary
    // test: has [1000,0,0,0], request 1000
    {
        let mut sgl = SGListBuilder::new()
            .with_sge(0x1000, 1000, 0_u32)
            .build();
        let payload = sgl.cut(1000).unwrap();
        assert_eq!(payload.get_length(), 1000);
        assert_eq!(sgl.data[0].len, 0);
        assert_eq!(sgl.cur_level, 1);
    }

    // test has [1000,0,0,0], request 1000
    {
        let mut sgl = SGListBuilder::new()
            .with_sge(0x1000, 1000, 0_u32)
            .build();
        let payload = sgl.cut(900).unwrap();
        assert_eq!(payload.get_length(), 900);
        assert_eq!(sgl.data[0].len, 100);
        assert_eq!(sgl.cur_level, 0);
    }

    // test has [1024,0,0,0], request 512,512
    {
        let mut sgl = SGListBuilder::new()
            .with_sge(0x1000, 1024, 0_u32)
            .build();
        let payload1 = sgl.cut(512).unwrap();
        let payload2 = sgl.cut(512).unwrap();
        assert_eq!(payload1.get_length(), 512);
        assert_eq!(payload2.get_length(), 512);
        assert_eq!(sgl.data[0].len, 0);
        assert_eq!(sgl.cur_level, 1);
        assert_eq!(payload1.get_sg_list().first().unwrap().data as u64, 0x1000);
        assert_eq!(
            payload2.get_sg_list().first().unwrap().data as u64,
            0x1000 + 512
        );
    }

    // test has [1024,2048,1124,100], require [100,2048,2048,100]
    {
        let mut sgl = SGListBuilder::new()
            .with_sge(0x1000, 1024, 0_u32)
            .with_sge(0x3000, 2048, 0_u32)
            .with_sge(0x6000, 1124, 0_u32)
            .with_sge(0x8000, 100, 0_u32)
            .build();

        let payload0 = sgl.cut(100).unwrap();
        let payload1 = sgl.cut(2048).unwrap();
        let payload2 = sgl.cut(2048).unwrap();
        let payload3 = sgl.cut(100).unwrap();
        assert_eq!(payload0.get_length(), 100);
        assert_eq!(payload1.get_length(), 2048);
        assert_eq!(payload2.get_length(), 2048);
        assert_eq!(payload3.get_length(), 100);
        assert_eq!(sgl.data[0].len, 0);
        assert_eq!(sgl.data[1].len, 0);
        assert_eq!(sgl.data[2].len, 0);
        assert_eq!(sgl.data[3].len, 0);
    }
}

#[test]
fn test_helper_cut_sgl_all() {
    let mut sgl = SGListBuilder::new()
        .with_sge(0x1000, 1024, 0_u32)
        .with_sge(0x2000, 1024, 0_u32)
        .with_sge(0x3000, 1024, 0_u32)
        .with_sge(0x4000, 1024, 0_u32)
        .build();
    let payload = sgl.cut_all_levels();
    assert_eq!(sgl.data[0].len, 0);
    assert_eq!(sgl.data[1].len, 0);
    assert_eq!(sgl.data[2].len, 0);
    assert_eq!(sgl.data[3].len, 0);
    assert_eq!(payload.get_length(), 4096);
}
