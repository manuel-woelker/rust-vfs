

pub trait WriteOp {}
pub trait MetadataOp {}

pub struct AllOps;
impl WriteOp for AllOps {}
impl MetadataOp for AllOps {}

pub struct ReadOnlyOps;
impl MetadataOp for ReadOnlyOps {}

