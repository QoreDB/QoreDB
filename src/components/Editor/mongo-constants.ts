export const MONGO_TEMPLATES = {
  find: `db.collection.find({
  // query filter
})`,
  findOne: `db.collection.findOne({
  // query filter
})`,
  aggregate: `db.collection.aggregate([
  { $match: { } },
  { $group: { _id: "$field", count: { $sum: 1 } } }
])`,
  insertOne: `db.collection.insertOne({
  // document
})`,
  updateOne: `db.collection.updateOne(
  { /* filter */ },
  { $set: { /* update */ } }
)`,
  updateMany: `db.collection.updateMany(
  { /* filter */ },
  { $set: { /* update */ } }
)`,
  deleteOne: `db.collection.deleteOne({
  // filter
})`,
};
